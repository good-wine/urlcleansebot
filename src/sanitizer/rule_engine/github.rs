use url::Url;

pub fn clean_github_url(url: &mut Url) -> bool {
    if let Some(host) = url.host_str()
        && host == "github.com"
    {
        let path_segments: Vec<String> = url
            .path_segments()
            .map(|s| s.map(String::from).collect())
            .unwrap_or_default();

        if path_segments.len() > 2 {
            let owner = &path_segments[0];
            let repo = &path_segments[1];
            let new_path = format!("/{}/{}", owner, repo);
            if url.path() != new_path {
                url.set_path(&new_path);
                url.set_query(None);
                url.set_fragment(None);
                return true;
            }
        }
    }
    false
}
