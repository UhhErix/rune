//! HTTP Live Streaming (HLS / M3U8) support: playlist parsing, adaptive bitrate selection,
//! HTTP(S) fetching with retry and cancellation, and bounded segment prefetching.

pub mod abr;
pub mod http;
pub mod playlist;
pub mod prefetch;
#[cfg(target_os = "windows")]
mod winhttp;

#[cfg(test)]
mod e2e_tests;
#[cfg(test)]
mod tests;

/// Summary of an HLS variant stream (from `#EXT-X-STREAM-INF`), exposed for telemetry and UI.
#[derive(Clone, Debug, PartialEq)]
pub struct HlsVariantInfo {
    pub bandwidth: u64,
    pub average_bandwidth: Option<u64>,
    pub resolution: Option<(u32, u32)>,
    pub codecs: Option<String>,
    pub frame_rate: Option<f64>,
}

impl From<&playlist::Variant> for HlsVariantInfo {
    fn from(v: &playlist::Variant) -> Self {
        Self {
            bandwidth: v.bandwidth,
            average_bandwidth: v.average_bandwidth,
            resolution: v.resolution,
            codecs: v.codecs.clone(),
            frame_rate: v.frame_rate,
        }
    }
}

/// Returns true when `url` is an `http://` or `https://` URL whose path ends with `.m3u8` or
/// `.m3u` (ignoring query strings and fragments, case-insensitive).
pub fn is_hls_url(url: &str) -> bool {
    let Some(rest) = strip_http_scheme(url) else {
        return false;
    };
    let without_frag = rest.split('#').next().unwrap_or(rest);
    let without_query = without_frag.split('?').next().unwrap_or(without_frag);
    // Skip authority ("host[:port]").
    let Some(slash) = without_query.find('/') else {
        return false;
    };
    let path = &without_query[slash..];
    let last_segment = path.rsplit('/').next().unwrap_or("");
    let Some((_, ext)) = last_segment.rsplit_once('.') else {
        return false;
    };
    ext.eq_ignore_ascii_case("m3u8") || ext.eq_ignore_ascii_case("m3u")
}

/// Resolves `reference` (absolute, protocol-relative, root-relative, query-only, or relative path)
/// against an `http://` or `https://` `base` URL.
pub fn resolve_url(base: &str, reference: &str) -> Result<String, String> {
    let reference = reference.trim();
    if reference.is_empty() {
        return Err("empty URI in HLS playlist".into());
    }
    if has_scheme(reference) {
        if strip_http_scheme(reference).is_some() {
            return Ok(reference.to_string());
        }
        return Err(format!(
            "unsupported URI scheme in HLS playlist: {reference}"
        ));
    }
    let (scheme, authority, base_path) = split_http_url(base)?;
    if let Some(after_slashes) = reference.strip_prefix("//") {
        return Ok(format!("{scheme}://{after_slashes}"));
    }
    if reference.starts_with('/') {
        let (path, suffix) = split_path_suffix(reference);
        return Ok(format!(
            "{scheme}://{authority}{}{suffix}",
            normalize_path(path)
        ));
    }
    if reference.starts_with('?') {
        return Ok(format!("{scheme}://{authority}{base_path}{reference}"));
    }
    if reference.starts_with('#') {
        return Ok(format!("{scheme}://{authority}{base_path}"));
    }
    let dir = match base_path.rfind('/') {
        Some(idx) => &base_path[..=idx],
        None => "/",
    };
    let (rel_path, suffix) = split_path_suffix(reference);
    let joined = format!("{dir}{rel_path}");
    Ok(format!(
        "{scheme}://{authority}{}{suffix}",
        normalize_path(&joined)
    ))
}

fn strip_http_scheme(url: &str) -> Option<&str> {
    if url.len() >= 8 && url[..8].eq_ignore_ascii_case("https://") {
        Some(&url[8..])
    } else if url.len() >= 7 && url[..7].eq_ignore_ascii_case("http://") {
        Some(&url[7..])
    } else {
        None
    }
}

fn has_scheme(s: &str) -> bool {
    let Some((scheme, _)) = s.split_once(':') else {
        return false;
    };
    let mut chars = scheme.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

fn split_http_url(url: &str) -> Result<(&str, &str, &str), String> {
    let (scheme, rest) = if url.len() >= 8 && url[..8].eq_ignore_ascii_case("https://") {
        ("https", &url[8..])
    } else if url.len() >= 7 && url[..7].eq_ignore_ascii_case("http://") {
        ("http", &url[7..])
    } else {
        return Err(format!("HLS base URL must be http:// or https://: {url}"));
    };
    let without_frag = rest.split('#').next().unwrap_or(rest);
    let without_query = without_frag.split('?').next().unwrap_or(without_frag);
    let (authority, path) = match without_query.find('/') {
        Some(idx) => (&without_query[..idx], &without_query[idx..]),
        None => (without_query, "/"),
    };
    if authority.is_empty() {
        return Err(format!("missing host in HLS URL: {url}"));
    }
    Ok((scheme, authority, path))
}

fn split_path_suffix(s: &str) -> (&str, &str) {
    let cut = s.find(['?', '#']).unwrap_or(s.len());
    (&s[..cut], &s[cut..])
}

fn normalize_path(path: &str) -> String {
    let trailing_slash = path.ends_with('/');
    let mut stack: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                stack.pop();
            }
            other => stack.push(other),
        }
    }
    let mut out = String::from("/");
    out.push_str(&stack.join("/"));
    if trailing_slash && out.len() > 1 {
        out.push('/');
    }
    out
}
