//! Preserve original responses privately; public provenance contains hashes, never article bodies.
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{fs, io, path::Path};

#[derive(Debug, Serialize)]
pub struct ArchiveReport {
    pub archived_files: usize,
    pub hashed_pages: usize,
    pub pages_without_markdown: usize,
}
fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn preserve(path: &Path, bytes: &[u8]) -> io::Result<()> {
    use std::io::Write;
    if path.exists() {
        if fs::read(path)? == bytes {
            return Ok(());
        }
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Private archive entry differs; refusing overwrite.",
        ));
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}
pub fn archive(
    raw_directory: &Path,
    private_directory: &Path,
    pages_file: &Path,
) -> io::Result<ArchiveReport> {
    if !private_directory.is_absolute() {
        return Err(io::Error::other(
            "Use an absolute durable private destination outside the Git worktree.",
        ));
    }
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| io::Error::other("Repository root unavailable"))?
        .canonicalize()?;
    // Resolve existing ancestors before creating a destination, including symlinks/junctions.
    let ancestor = private_directory
        .ancestors()
        .find(|p| p.exists())
        .ok_or_else(|| io::Error::other("Private destination has no existing ancestor"))?
        .canonicalize()?;
    if ancestor.starts_with(&repository) || ancestor.ancestors().any(|p| p.join(".git").exists()) {
        return Err(io::Error::other(
            "Do not archive publisher text inside a Git worktree.",
        ));
    }
    let original_pages = fs::read(pages_file)?;
    let mut pages: Value = serde_json::from_slice(&original_pages)
        .map_err(|_| io::Error::other("Invalid retained pages JSON"))?;
    if !pages.is_array() {
        return Err(io::Error::other("Expected retained pages array"));
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(raw_directory)? {
        let entry = entry?;
        if entry.file_type()?.is_file() && entry.path().extension().is_some_and(|s| s == "json") {
            files.push(entry.path());
        }
    }
    if let Some(parent) = raw_directory.parent() {
        for name in ["live_lista.json", "verified.json"] {
            let path = parent.join(name);
            if path.is_file() && !files.contains(&path) {
                files.push(path);
            }
        }
    }
    files.sort();
    fs::create_dir_all(private_directory)?;
    let mut manifest = Vec::new();
    let mut hashed = 0;
    let mut without = 0;
    for path in files {
        if fs::metadata(&path)?.len() > 16 * 1024 * 1024 {
            return Err(io::Error::other(
                "Unexpected oversized capture; inspect before archiving.",
            ));
        }
        let bytes = fs::read(&path)?;
        let raw: Value = serde_json::from_slice(&bytes)
            .map_err(|_| io::Error::other("Invalid raw response JSON"))?;
        let filename = path
            .file_name()
            .ok_or_else(|| io::Error::other("Missing capture filename"))?;
        preserve(&private_directory.join(filename), &bytes)?;
        let md = raw.pointer("/markdown/data").and_then(Value::as_str);
        let url = raw
            .get("_requested_url")
            .or(raw.get("url"))
            .and_then(Value::as_str);
        let md_hash = md.map(|s| digest(s.as_bytes()));
        if let Some(page) = pages
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|p| p.get("url").and_then(Value::as_str) == url && url.is_some())
        {
            if let Some(hash) = &md_hash {
                if raw.get("request_id") != page.get("request_id")
                    || raw.get("request_id").is_none_or(Value::is_null)
                {
                    return Err(io::Error::other(
                        "Raw capture request ID differs from retained provenance; do not hash a recapture as the original.",
                    ));
                }
                page["markdown_sha256"] = json!(hash);
                page["raw_response_sha256"] = json!(digest(&bytes));
                page["markdown_hash_basis"] = json!("sha256_exact_decoded_utf8_no_normalization");
                hashed += 1;
            } else {
                without += 1;
            }
        }
        manifest.push(json!({"file":filename.to_string_lossy(),"response_sha256":digest(&bytes),"markdown_sha256":md_hash,"request_id":raw.get("request_id"),"url":url}));
    }
    if hashed == 0 {
        return Err(io::Error::other(
            "No matching original Markdown captures found; retained provenance was not changed.",
        ));
    }
    preserve(
        &private_directory.join("pages-before-hashes.json"),
        &original_pages,
    )?;
    // All IDs were checked before publishing any hash changes.
    fs::write(
        pages_file,
        serde_json::to_vec_pretty(&pages).map_err(io::Error::other)?,
    )?;
    preserve(
        &private_directory.join("archive-manifest.json"),
        &serde_json::to_vec_pretty(&manifest).map_err(io::Error::other)?,
    )?;
    Ok(ArchiveReport {
        archived_files: manifest.len(),
        hashed_pages: hashed,
        pages_without_markdown: without,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_request_and_exact_unicode_bytes_are_required() {
        let base = std::env::temp_dir().join(format!(
            "emberwatch-archive-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(base.join("raw")).unwrap();
        let md = "Pio\u{301}dão\r\n  fonte";
        let raw = json!({"url":"https://example.org/synthetic","request_id":"fixture-original","markdown":{"data":md}});
        fs::write(
            base.join("raw/scrape-fixture.json"),
            serde_json::to_vec(&raw).unwrap(),
        )
        .unwrap();
        fs::write(
            base.join("pages.json"),
            serde_json::to_vec(
                &json!([{"url":"https://example.org/synthetic","request_id":"fixture-original"}]),
            )
            .unwrap(),
        )
        .unwrap();
        let report = archive(
            &base.join("raw"),
            &base.join("private"),
            &base.join("pages.json"),
        )
        .unwrap();
        assert_eq!(report.hashed_pages, 1);
        let pages: Value =
            serde_json::from_slice(&fs::read(base.join("pages.json")).unwrap()).unwrap();
        assert_eq!(pages[0]["markdown_sha256"], digest(md.as_bytes()));
        assert_ne!(
            pages[0]["markdown_sha256"],
            digest(crate::accuracy::normalize(md).as_bytes())
        );
        let mut recaptured = raw;
        recaptured["request_id"] = json!("fixture-recapture");
        fs::write(
            base.join("raw/scrape-fixture.json"),
            serde_json::to_vec(&recaptured).unwrap(),
        )
        .unwrap();
        assert!(
            archive(
                &base.join("raw"),
                &base.join("another-private"),
                &base.join("pages.json")
            )
            .is_err()
        );
        let root = base.canonicalize().unwrap();
        let temp = std::env::temp_dir().canonicalize().unwrap();
        assert!(root.starts_with(&temp) && root != temp);
        fs::remove_dir_all(root).unwrap();
    }
}
