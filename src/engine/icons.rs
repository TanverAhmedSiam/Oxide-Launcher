//! Project logos. Downloaded once, shrunk to at most 64x64 and cached on disk, so they
//! cost little memory (about 16 KB each) and work offline afterwards.
use super::trace;
use anyhow::{bail, Result};
use sha1::{Digest, Sha1};
use std::io::Cursor;
use std::path::{Path, PathBuf};

pub struct Icon {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

fn cache_file(dir: &Path, url: &str) -> PathBuf {
    let mut h = Sha1::new();
    h.update(url.as_bytes());
    dir.join(format!("{}.png", hex::encode(h.finalize())))
}

pub async fn load_icon(client: &reqwest::Client, cache_dir: &Path, url: &str) -> Result<Icon> {
    trace::log(&format!("icon: start {url}"));
    let file = cache_file(cache_dir, url);
    if let Ok(bytes) = tokio::fs::read(&file).await {
        if let Ok(img) = image::load_from_memory(&bytes) {
            let t = img.to_rgba8();
            let (w, h) = t.dimensions();
            trace::log(&format!("icon: from cache {w}x{h}"));
            return Ok(Icon { w: w as usize, h: h as usize, rgba: t.into_raw() });
        }
    }
    let bytes = client
        .get(url)
        .send()
        .await
        .and_then(|r| r.error_for_status())?
        .bytes()
        .await?;
    if bytes.len() > 5_000_000 {
        bail!("icon too large");
    }
    let bytes = bytes.to_vec();
    let (icon, png) = tokio::task::spawn_blocking(move || -> Result<(Icon, Vec<u8>)> {
        let img = image::load_from_memory(&bytes)?;
        let small = img.thumbnail(64, 64).to_rgba8();
        let (w, h) = small.dimensions();
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(small.clone())
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)?;
        Ok((Icon { w: w as usize, h: h as usize, rgba: small.into_raw() }, png))
    })
    .await??;
    trace::log(&format!("icon: decoded {}x{}", icon.w, icon.h));
    let _ = tokio::fs::create_dir_all(cache_dir).await;
    let _ = tokio::fs::write(&file, png).await;
    Ok(icon)
}
