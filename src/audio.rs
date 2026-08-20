//! Self-contained ACX audio pipeline: decode MP3, measure loudness, apply the
//! gain / limiter / room-tone chain, resample, and encode CBR MP3 — no ffmpeg.

use anyhow::{anyhow, bail, Context, Result};
use std::path::Path;

// ---- ACX pipeline constants (kept identical to the reference bash script) ----
pub const TARGET_RMS_DB: f32 = -20.0; // center of the ACX -23..-18 window
pub const PEAK_LIMIT: f32 = 0.668; // linear ceiling ~= -3.5 dBFS
pub const LEAD_SILENCE_S: f32 = 0.5; // room tone at head
pub const TAIL_SILENCE_S: f32 = 2.0; // room tone at tail
pub const OUT_SAMPLE_RATE: u32 = 44_100;
pub const OUT_BITRATE_KBPS: u32 = 192;

/// Decoded stereo audio as interleaved-by-frame `[left, right]` samples in
/// `[-1.0, 1.0]`.
pub struct Audio {
    pub sample_rate: u32,
    pub frames: Vec<[f32; 2]>,
}

impl Audio {
    pub fn duration_secs(&self) -> f64 {
        self.frames.len() as f64 / self.sample_rate as f64
    }
}

/// Loudness measurement matching ffmpeg's `volumedetect`.
pub struct Measure {
    pub mean_db: f32, // RMS over all samples of all channels, pooled
    pub peak_db: f32, // max |sample|
    pub duration_s: f64,
}

/// RMS/peak over every sample of both channels (what `volumedetect` reports).
pub fn measure(audio: &Audio) -> Measure {
    let mut sumsq = 0.0f64;
    let mut peak = 0.0f32;
    let n = (audio.frames.len() * 2).max(1) as f64;
    for fr in &audio.frames {
        for &s in fr {
            sumsq += (s as f64) * (s as f64);
            let a = s.abs();
            if a > peak {
                peak = a;
            }
        }
    }
    let mean_db = if sumsq > 0.0 {
        10.0 * (sumsq / n).log10() as f32
    } else {
        f32::NEG_INFINITY
    };
    let peak_db = if peak > 0.0 {
        20.0 * peak.log10()
    } else {
        f32::NEG_INFINITY
    };
    Measure {
        mean_db,
        peak_db,
        duration_s: audio.duration_secs(),
    }
}

// ---- decode (symphonia, pure Rust) ------------------------------------------

/// Decode an MP3 to stereo f32. Mono is duplicated to both channels. Gapless
/// decoding is enabled so encoder delay/padding does not inflate the duration.
pub fn decode_mp3(path: &Path) -> Result<Audio> {
    use symphonia::core::audio::SampleBuffer;
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("mp3");

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions {
                enable_gapless: true,
                ..Default::default()
            },
            &MetadataOptions::default(),
        )
        .with_context(|| format!("{} is not a decodable MP3", path.display()))?;
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| anyhow!("no audio track in {}", path.display()))?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("no MP3 decoder available")?;

    let mut frames: Vec<[f32; 2]> = Vec::new();
    let mut sample_rate = 0u32;
    let mut sbuf: Option<SampleBuffer<f32>> = None;

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break
            }
            Err(e) => return Err(anyhow!("decode error: {e}")),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue, // skip bad frame
            Err(e) => return Err(anyhow!("decode error: {e}")),
        };
        let spec = *decoded.spec();
        sample_rate = spec.rate;
        let chans = spec.channels.count();
        if sbuf.is_none() {
            sbuf = Some(SampleBuffer::new(decoded.capacity() as u64, spec));
        }
        let sb = sbuf.as_mut().unwrap();
        sb.copy_interleaved_ref(decoded);
        let samples = sb.samples();
        match chans {
            1 => {
                for &s in samples {
                    frames.push([s, s]);
                }
            }
            _ => {
                // Interleaved; take the first two channels (MP3 is mono/stereo).
                for chunk in samples.chunks(chans) {
                    frames.push([chunk[0], chunk[1]]);
                }
            }
        }
    }

    if sample_rate == 0 {
        bail!("{} produced no audio", path.display());
    }
    Ok(Audio {
        sample_rate,
        frames,
    })
}

// ---- DSP --------------------------------------------------------------------

/// Apply a linear gain to every sample.
pub fn apply_gain(audio: &mut Audio, gain_lin: f32) {
    for fr in &mut audio.frames {
        fr[0] *= gain_lin;
        fr[1] *= gain_lin;
    }
}

/// Look-ahead peak limiter. Guarantees output peak <= `ceiling` by dropping the
/// gain envelope ahead of any overshoot (instant attack via a look-ahead
/// windowed minimum) and recovering with an exponential release. Stereo-linked
/// (one gain for both channels) so the image is preserved.
pub fn limit(audio: &mut Audio, ceiling: f32, sample_rate: u32) {
    let n = audio.frames.len();
    if n == 0 {
        return;
    }
    let lookahead = ((0.005 * sample_rate as f32) as usize).max(1); // ~5 ms
    // Per-frame required gain (1.0 where already under the ceiling).
    let mut req = vec![1.0f32; n];
    for (i, fr) in audio.frames.iter().enumerate() {
        let amp = fr[0].abs().max(fr[1].abs());
        if amp > ceiling {
            req[i] = ceiling / amp;
        }
    }
    // Look-ahead windowed minimum: gain at i must satisfy every peak within the
    // next `lookahead` frames, so the envelope is already down when it arrives.
    let mut win_min = vec![1.0f32; n];
    {
        // Monotonic deque over indices for a sliding-window minimum.
        use std::collections::VecDeque;
        let mut dq: VecDeque<usize> = VecDeque::new();
        // Process from the right so the window covers [i, i+lookahead].
        for i in (0..n).rev() {
            while let Some(&b) = dq.back() {
                if req[b] >= req[i] {
                    dq.pop_back();
                } else {
                    break;
                }
            }
            dq.push_back(i);
            let front = *dq.front().unwrap();
            if front > i + lookahead {
                dq.pop_front();
            }
            win_min[i] = req[*dq.front().unwrap()];
        }
    }
    let release_coef = (-1.0 / (0.05 * sample_rate as f32)).exp(); // ~50 ms
    let mut env = 1.0f32;
    for i in 0..n {
        let target = win_min[i];
        if target < env {
            env = target; // instant attack (look-ahead gave the headroom)
        } else {
            env = target + (env - target) * release_coef; // exponential release
        }
        audio.frames[i][0] *= env;
        audio.frames[i][1] *= env;
    }
}

/// Prepend/append digital-silence room tone (in seconds, at the current rate).
pub fn pad_silence(audio: &mut Audio, lead_s: f32, tail_s: f32) {
    let lead = (lead_s * audio.sample_rate as f32).round() as usize;
    let tail = (tail_s * audio.sample_rate as f32).round() as usize;
    let mut out = Vec::with_capacity(lead + audio.frames.len() + tail);
    out.extend(std::iter::repeat([0.0f32, 0.0]).take(lead));
    out.append(&mut audio.frames);
    out.extend(std::iter::repeat([0.0f32, 0.0]).take(tail));
    audio.frames = out;
}

/// Resample to `OUT_SAMPLE_RATE` if needed (high-quality, pure Rust).
pub fn resample_to_out(audio: &mut Audio) -> Result<()> {
    if audio.sample_rate == OUT_SAMPLE_RATE {
        return Ok(());
    }
    use rubato::{
        Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
    };
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };
    let ratio = OUT_SAMPLE_RATE as f64 / audio.sample_rate as f64;
    let mut resampler = SincFixedIn::<f32>::new(ratio, 2.0, params, audio.frames.len().max(1), 2)
        .map_err(|e| anyhow!("resampler init: {e}"))?;
    let left: Vec<f32> = audio.frames.iter().map(|f| f[0]).collect();
    let right: Vec<f32> = audio.frames.iter().map(|f| f[1]).collect();
    let out = resampler
        .process(&[left, right], None)
        .map_err(|e| anyhow!("resample: {e}"))?;
    let (lo, ro) = (&out[0], &out[1]);
    audio.frames = lo.iter().zip(ro).map(|(&l, &r)| [l, r]).collect();
    audio.sample_rate = OUT_SAMPLE_RATE;
    Ok(())
}

// ---- encode (libmp3lame, compiled in) ---------------------------------------

/// Encode stereo audio to a CBR MP3 at `OUT_BITRATE_KBPS`. Assumes the audio is
/// already at `OUT_SAMPLE_RATE`.
pub fn encode_mp3_cbr(audio: &Audio, out: &Path) -> Result<()> {
    use mp3lame_encoder::{Bitrate, Builder, DualPcm, FlushNoGap, Quality};

    let mut builder = Builder::new().ok_or_else(|| anyhow!("mp3lame: builder init failed"))?;
    builder
        .set_num_channels(2)
        .map_err(|e| anyhow!("mp3lame channels: {e:?}"))?;
    builder
        .set_sample_rate(OUT_SAMPLE_RATE)
        .map_err(|e| anyhow!("mp3lame sample rate: {e:?}"))?;
    builder
        .set_brate(Bitrate::Kbps192)
        .map_err(|e| anyhow!("mp3lame bitrate: {e:?}"))?;
    builder
        .set_quality(Quality::Best)
        .map_err(|e| anyhow!("mp3lame quality: {e:?}"))?;
    let mut encoder = builder
        .build()
        .map_err(|e| anyhow!("mp3lame build: {e:?}"))?;

    // f32 [-1,1] -> i16 PCM, split into planar L/R.
    let n = audio.frames.len();
    let mut left = Vec::with_capacity(n);
    let mut right = Vec::with_capacity(n);
    for fr in &audio.frames {
        left.push(to_i16(fr[0]));
        right.push(to_i16(fr[1]));
    }

    let mut mp3: Vec<u8> = Vec::with_capacity(n + n / 4 + 7200);
    let input = DualPcm {
        left: &left,
        right: &right,
    };
    let size = encoder
        .encode(input, mp3.spare_capacity_mut())
        .map_err(|e| anyhow!("mp3lame encode: {e:?}"))?;
    unsafe { mp3.set_len(mp3.len() + size) };
    let size = encoder
        .flush::<FlushNoGap>(mp3.spare_capacity_mut())
        .map_err(|e| anyhow!("mp3lame flush: {e:?}"))?;
    unsafe { mp3.set_len(mp3.len() + size) };

    std::fs::write(out, &mp3).with_context(|| format!("writing {}", out.display()))?;
    Ok(())
}

fn to_i16(s: f32) -> i16 {
    (s.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

// ---- high-level convert -----------------------------------------------------

/// Full ACX conversion: measure -> gain to target RMS -> limit peaks -> add
/// room tone -> resample -> encode CBR MP3. Returns the post-convert measurement
/// of the produced file's PCM (before encode).
pub fn convert_file(src: &Path, dst: &Path) -> Result<Measure> {
    let mut audio = decode_mp3(src)?;
    let m = measure(&audio);
    let gain_db = TARGET_RMS_DB - m.mean_db;
    let gain_lin = 10f32.powf(gain_db / 20.0);
    apply_gain(&mut audio, gain_lin);
    let sr = audio.sample_rate;
    limit(&mut audio, PEAK_LIMIT, sr);
    resample_to_out(&mut audio)?;
    pad_silence(&mut audio, LEAD_SILENCE_S, TAIL_SILENCE_S);
    encode_mp3_cbr(&audio, dst)?;
    // Copy ID3 tags (best-effort, parity with ffmpeg -map_metadata 0).
    let _ = copy_id3(src, dst);
    Ok(measure(&audio))
}

fn copy_id3(src: &Path, dst: &Path) -> Result<()> {
    match id3::Tag::read_from_path(src) {
        Ok(tag) => tag
            .write_to_path(dst, id3::Version::Id3v24)
            .map_err(|e| anyhow!("id3 write: {e}")),
        Err(_) => Ok(()), // no tag / unreadable -> nothing to copy
    }
}
