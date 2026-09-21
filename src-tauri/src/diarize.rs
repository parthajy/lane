//! Speaker diarisation for the other side of a meeting: which stretches of
//! `system.wav` were said by the same voice. Runs on this Mac with an ONNX
//! toolkit and two small models fetched once into the data folder (opt-in,
//! Settings → Meetings). Voices get labels ("Speaker 1"); the user gives
//! them names on the Meetings page.

use std::path::{Path, PathBuf};
use std::process::Command;

const TOOLKIT_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/v1.13.8/sherpa-onnx-v1.13.8-osx-arm64-shared-no-tts.tar.bz2";
const TOOLKIT_DIR: &str = "sherpa-onnx-v1.13.8-osx-arm64-shared-no-tts";
const SEGMENTATION_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-segmentation-models/sherpa-onnx-pyannote-segmentation-3-0.tar.bz2";
const SEGMENTATION_DIR: &str = "sherpa-onnx-pyannote-segmentation-3-0";
const EMBEDDING_URL: &str = "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/nemo_en_titanet_small.onnx";
const EMBEDDING_FILE: &str = "nemo_en_titanet_small.onnx";

pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("diarize")
}

fn binary(d: &Path) -> PathBuf {
    d.join(TOOLKIT_DIR).join("bin").join("sherpa-onnx-offline-speaker-diarization")
}
fn segmentation(d: &Path) -> PathBuf {
    d.join(SEGMENTATION_DIR).join("model.onnx")
}
fn embedding(d: &Path) -> PathBuf {
    d.join(EMBEDDING_FILE)
}

pub fn ready(data_dir: &Path) -> bool {
    let d = dir(data_dir);
    binary(&d).is_file() && segmentation(&d).is_file() && embedding(&d).is_file()
}

fn fetch(url: &str, dest: &Path) -> Result<(), String> {
    let part = dest.with_extension("part");
    let resp = ureq::get(url).timeout(std::time::Duration::from_secs(600)).call().map_err(|e| format!("download failed: {e}"))?;
    let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    std::io::copy(&mut resp.into_reader(), &mut file).map_err(|e| e.to_string())?;
    std::fs::rename(&part, dest).map_err(|e| e.to_string())
}

/// Fetch the toolkit and models if missing. About 60 MB once.
pub fn ensure(data_dir: &Path, mut status: impl FnMut(&str)) -> Result<(), String> {
    if cfg!(not(all(target_os = "macos", target_arch = "aarch64"))) {
        return Err("speaker separation is only available on Apple silicon for now".into());
    }
    let d = dir(data_dir);
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    if !binary(&d).is_file() {
        status("downloading the speaker toolkit (20 MB)");
        let tar = d.join("toolkit.tar.bz2");
        fetch(TOOLKIT_URL, &tar)?;
        let ok = Command::new("tar").args(["xjf", &tar.display().to_string(), "-C", &d.display().to_string()]).status().map_err(|e| e.to_string())?.success();
        let _ = std::fs::remove_file(&tar);
        if !ok || !binary(&d).is_file() {
            return Err("could not unpack the speaker toolkit".into());
        }
    }
    if !segmentation(&d).is_file() {
        status("downloading the speaker segmentation model (6 MB)");
        let tar = d.join("segmentation.tar.bz2");
        fetch(SEGMENTATION_URL, &tar)?;
        let ok = Command::new("tar").args(["xjf", &tar.display().to_string(), "-C", &d.display().to_string()]).status().map_err(|e| e.to_string())?.success();
        let _ = std::fs::remove_file(&tar);
        if !ok || !segmentation(&d).is_file() {
            return Err("could not unpack the segmentation model".into());
        }
    }
    if !embedding(&d).is_file() {
        status("downloading the speaker voice model (40 MB)");
        fetch(EMBEDDING_URL, &embedding(&d))?;
    }
    Ok(())
}

/// (start ms, end ms, speaker index) for a 16 kHz mono WAV.
pub fn diarize(data_dir: &Path, wav: &Path) -> Result<Vec<(i64, i64, usize)>, String> {
    let d = dir(data_dir);
    if !ready(data_dir) {
        return Err("speaker toolkit missing".into());
    }
    if std::fs::metadata(wav).map(|m| m.len()).unwrap_or(0) < 64_000 {
        return Ok(vec![]);
    }
    let out = Command::new(binary(&d))
        .arg(format!("--segmentation.pyannote-model={}", segmentation(&d).display()))
        .arg(format!("--embedding.model={}", embedding(&d).display()))
        .arg("--clustering.cluster-threshold=0.8")
        .arg("--segmentation.num-threads=2")
        .arg("--embedding.num-threads=2")
        .arg("--print-args=false")
        .arg(wav)
        .output()
        .map_err(|e| format!("could not run the speaker toolkit: {e}"))?;
    if !out.status.success() {
        return Err(format!("speaker toolkit failed: {}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("")));
    }
    Ok(parse(&String::from_utf8_lossy(&out.stdout)))
}

/// Lines look like `0.318 -- 6.865 speaker_00`.
pub fn parse(text: &str) -> Vec<(i64, i64, usize)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() != 4 || parts[1] != "--" {
            continue;
        }
        let (Ok(a), Ok(b)) = (parts[0].parse::<f64>(), parts[2].parse::<f64>()) else { continue };
        let Some(idx) = parts[3].strip_prefix("speaker_").and_then(|n| n.parse::<usize>().ok()) else { continue };
        out.push(((a * 1000.0) as i64, (b * 1000.0) as i64, idx));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_toolkit_output() {
        let v = parse("Started\n0.318 -- 6.865 speaker_00\n7.017 -- 10.747 speaker_01\nnoise line\n");
        assert_eq!(v, vec![(318, 6865, 0), (7017, 10747, 1)]);
    }
}
