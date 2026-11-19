use anyhow::{bail, Context, Result};
use futures_util::{stream, StreamExt};
use sha1::{Digest, Sha1};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct Job {
    pub url: String,
    pub path: PathBuf,
    pub sha1: Option<String>,
}

pub fn client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent("OxideLauncher/1.0")
        .pool_max_idle_per_host(2)
        .build()?)
}

async fn sha1_of(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path).await?;
    let mut h = Sha1::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

/// Download one file. Skips it if already present and the hash matches.
/// Streams to disk (constant memory) via a .part file, verifying SHA-1 on the way.
pub async fn fetch(client: &reqwest::Client, job: &Job) -> Result<()> {
    fetch_cb(client, job, &|_, _| {}).await
}

/// Same as `fetch`, but reports (bytes so far, total bytes or 0 if unknown).
pub async fn fetch_cb(
    client: &reqwest::Client,
    job: &Job,
    on_bytes: &(dyn Fn(u64, u64) + Sync),
) -> Result<()> {
    if job.path.exists() {
        match &job.sha1 {
            Some(want) => {
                let ok = sha1_of(&job.path)
                    .await
                    .map(|h| h.eq_ignore_ascii_case(want))
                    .unwrap_or(false);
                if ok {
                    return Ok(());
                }
            }
            None => return Ok(()),
        }
    }
    if let Some(p) = job.path.parent() {
        fs::create_dir_all(p).await?;
    }
    let mut tmp = job.path.clone().into_os_string();
    tmp.push(".part");
    let tmp = PathBuf::from(tmp);

    let resp = client
        .get(&job.url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .with_context(|| format!("GET {}", job.url))?;

    let mut file = fs::File::create(&tmp).await?;
    let mut hasher = Sha1::new();
    let total = resp.content_length().unwrap_or(0);
    let (mut got, mut last) = (0u64, 0u64);
    let mut body = resp.bytes_stream();
    while let Some(chunk) = body.next().await {
        let c = chunk?;
        hasher.update(&c);
        file.write_all(&c).await?;
        got += c.len() as u64;
        if got - last >= 64 * 1024 {
            last = got;
            on_bytes(got, total);
        }
    }
    on_bytes(got, total.max(got));
    file.flush().await?;
    drop(file);

    if let Some(want) = &job.sha1 {
        let got = hex::encode(hasher.finalize());
        if !got.eq_ignore_ascii_case(want) {
            let _ = fs::remove_file(&tmp).await;
            bail!("checksum mismatch for {}", job.url);
        }
    }
    fs::rename(&tmp, &job.path).await?;
    Ok(())
}

pub async fn fetch_retry(client: &reqwest::Client, job: &Job) -> Result<()> {
    fetch_retry_cb(client, job, &|_, _| {}).await
}

pub async fn fetch_retry_cb(
    client: &reqwest::Client,
    job: &Job,
    on_bytes: &(dyn Fn(u64, u64) + Sync),
) -> Result<()> {
    let mut last = None;
    for attempt in 0..3u64 {
        match fetch_cb(client, job, on_bytes).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                last = Some(e);
                tokio::time::sleep(Duration::from_millis(500 * (attempt + 1))).await;
            }
        }
    }
    Err(last.unwrap())
}

/// Download many files with bounded concurrency (keeps RAM and sockets low).
pub async fn fetch_all(
    client: &reqwest::Client,
    mut jobs: Vec<Job>,
    on_progress: impl Fn(usize, usize),
) -> Result<()> {
    let mut seen = HashSet::new();
    jobs.retain(|j| seen.insert(j.path.clone())); // never write the same file twice at once
    let total = jobs.len();
    let mut done = 0usize;
    on_progress(0, total);
    let mut results = stream::iter(jobs.into_iter().map(|j| {
        let c = client.clone();
        async move { fetch_retry(&c, &j).await }
    }))
    .buffer_unordered(16);
    while let Some(r) = results.next().await {
        r?;
        done += 1;
        if done % 20 == 0 || done == total {
            on_progress(done, total);
        }
    }
    Ok(())
}
