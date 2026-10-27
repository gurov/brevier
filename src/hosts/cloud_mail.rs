//! Public Cloud Mail folders and files are a JavaScript app shell. The public
//! API supplies folder entries and a temporary download URL for file contents.

use url::Url;

use crate::json::{self, Value};

/// The decoded share path in a public URL.
pub fn public_weblink(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    if parsed.host_str()? != "cloud.mail.ru" {
        return None;
    }
    let mut parts = parsed.path_segments()?;
    if parts.next()? != "public" {
        return None;
    }
    let parts: Vec<String> = parts.map(crate::address::percent_decoded).collect();
    (parts.len() >= 2).then(|| parts.join("/"))
}

/// API URL for a public Cloud Mail share, including a path into a folder.
/// Other Cloud Mail pages keep the ordinary HTML reading path.
pub fn folder_api(url: &str) -> Option<String> {
    let share = public_weblink(url)?;
    let mut api = Url::parse("https://cloud.mail.ru/api/v2/folder").ok()?;
    api.query_pairs_mut().append_pair("weblink", &share);
    Some(api.into())
}

/// Metadata request for a public PDF. The extension avoids an extra API
/// request for every folder share.
pub fn pdf_api(url: &str) -> Option<String> {
    let share = public_weblink(url)?;
    let name = share.rsplit('/').next()?;
    name.to_ascii_lowercase().ends_with(".pdf").then_some(())?;
    let mut api = Url::parse("https://cloud.mail.ru/api/v2/file").ok()?;
    api.query_pairs_mut().append_pair("weblink", &share);
    Some(api.into())
}

/// Dispatcher request that returns the current public download shard.
pub fn dispatcher_api() -> String {
    let mut api = Url::parse("https://cloud.mail.ru/api/v2/dispatcher")
        .expect("Cloud Mail dispatcher URL is valid");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string();
    api.query_pairs_mut()
        .append_pair("api", "2")
        .append_pair("email", "")
        .append_pair("_", &now);
    api.into()
}

/// Download URL for the shared file, built from the current dispatcher shard.
pub fn download_url(url: &str, dispatcher_json: &str) -> Option<String> {
    let share = public_weblink(url)?;
    let root = json::parse(dispatcher_json)?;
    let body = root.get("body")?;
    let prefix = body.get("weblink_get")?.as_array().first()?.text("url")?;
    let mut direct = Url::parse(prefix).ok()?;
    let host = direct.host_str()?;
    if direct.scheme() != "https"
        || !(host.ends_with(".cloud.mail.ru")
            || host.ends_with(".datacloudmail.ru")
            || host.ends_with(".cldmail.ru"))
    {
        return None;
    }
    {
        let mut path = direct.path_segments_mut().ok()?;
        for part in share.split('/') {
            path.push(part);
        }
    }
    Some(direct.into())
}

/// Name if the metadata response describes a PDF file.
pub fn pdf_name(metadata_json: &str) -> Option<String> {
    let root = json::parse(metadata_json)?;
    if !matches!(root.get("status"), Some(Value::Number(n)) if n == "200") {
        return None;
    }
    let body = root.get("body")?;
    if body.text("type")? != "file" {
        return None;
    }
    let name = body.text("name")?;
    name.to_ascii_lowercase()
        .ends_with(".pdf")
        .then(|| name.to_owned())
}

/// Extract text with Brevier's typography from a downloaded PDF. PDF parsers
/// can panic on malformed files, so keep that failure inside this boundary.
pub fn pdf_markdown(bytes: &[u8], title: &str) -> Option<String> {
    let text = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pdf_extract::extract_text_from_mem(bytes)
    }))
    .ok()?
    .ok()?;
    let pages = text
        .split('\u{c}')
        .map(str::trim)
        .filter(|page| !page.is_empty())
        .collect::<Vec<_>>();
    if pages.is_empty() {
        return None;
    }
    Some(format!(
        "# {}\n\n{}\n",
        crate::feed::inline(title),
        pages.join("\n\n---\n\n")
    ))
}

/// Turn the public folder response into Atom, which Brevier already renders
/// as a link listing. Invalid, private, and non-folder responses fall back to
/// the site's HTML shell and its normal extraction error.
pub fn atom(json_text: &str, page_url: &str) -> Option<String> {
    let root = json::parse(json_text)?;
    // Cloud Mail sends status as a number.
    if !matches!(root.get("status"), Some(Value::Number(n)) if n == "200") {
        return None;
    }
    let body = root.get("body")?;
    if body.text("type")? != "folder" {
        return None;
    }
    let title = body.text("name")?;
    let entries = body.get("list")?.as_array();
    let mut out = format!(
        "<feed xmlns=\"http://www.w3.org/2005/Atom\"><title>{}</title><link rel=\"self\" href=\"{}\" />",
        xml(title),
        xml(page_url),
    );
    for entry in entries {
        let (Some(name), Some(weblink)) = (entry.text("name"), entry.text("weblink")) else {
            continue;
        };
        let Some(link) = public_url(weblink) else {
            continue;
        };
        out.push_str(&format!(
            "<entry><title>{}</title><link href=\"{}\" /></entry>",
            xml(name),
            xml(&link),
        ));
    }
    out.push_str("</feed>");
    Some(out)
}

fn public_url(weblink: &str) -> Option<String> {
    let mut url = Url::parse("https://cloud.mail.ru/").ok()?;
    {
        let mut path = url.path_segments_mut().ok()?;
        path.clear().push("public");
        for part in weblink.split('/') {
            if part.is_empty() {
                return None;
            }
            path.push(part);
        }
    }
    Some(url.into())
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
