//! acxorcist — batch-convert MP3s into ACX-compliant audiobook files.
//!
//! A single self-contained binary: MP3 decode, loudness/peak analysis, the
//! gain / limiter / room-tone mastering chain, resampling, and CBR MP3 encoding
//! are all done in-process (symphonia + libmp3lame compiled in). No ffmpeg or
//! any other external tool is required at runtime.

pub mod audio;

pub use audio::{convert_file, decode_mp3, measure, Audio, Measure};
