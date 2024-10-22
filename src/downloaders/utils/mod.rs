use anyhow::{Context, Result};
use tl::{HTMLTag, Parser, VDom};

pub(crate) mod common_url_pattern_donwloader;

#[inline]
pub fn is_proper_authority<S: AsRef<str>>(uri: &hyper::Uri, authority: S) -> bool {
    uri.authority()
        .map(|a| a.as_str() == authority.as_ref())
        .unwrap_or(false)
}

#[inline]
pub fn is_supported_scheme(uri: &hyper::Uri) -> bool {
    match uri.scheme_str() {
        Some("http") => true,
        Some("https") => true,
        _ => false,
    }
}

#[inline]
pub fn merge_uris(main: &hyper::Uri, fallback: &hyper::Uri) -> hyper::Uri {
    use hyper::http::uri::Parts;

    let mut parts = Parts::default();
    parts.scheme = main.scheme().or_else(|| fallback.scheme()).cloned();
    parts.authority = main.authority().or_else(|| fallback.authority()).cloned();
    parts.path_and_query = main
        .path_and_query()
        .or_else(|| fallback.path_and_query())
        .cloned();

    hyper::Uri::from_parts(parts).expect("failed to merge uris")
}

#[async_trait::async_trait]
pub trait CollectResponse {
    async fn collect_response(self) -> Result<Vec<u8>>;
}

#[async_trait::async_trait]
impl CollectResponse for hyper::Response<hyper::body::Incoming> {
    async fn collect_response(mut self) -> Result<Vec<u8>> {
        use http_body_util::BodyExt;

        let mut page = Vec::new();
        while let Some(next) = self.frame().await {
            if let Some(chunck) = next?.data_ref() {
                page.extend(chunck);
            }
        }

        Ok(page)
    }
}

pub trait GetHtmlTag<'a> {
    fn get_html_tag<'b>(&'b self) -> Result<&'b HTMLTag<'a>>;
}

impl<'a> GetHtmlTag<'a> for VDom<'a> {
    fn get_html_tag<'b>(&'b self) -> Result<&'b HTMLTag<'a>> {
        let parser = self.parser();
        self.query_selector("html")
            .and_then(|mut q| q.next())
            .and_then(|node| node.get(parser))
            .and_then(|node| node.as_tag())
            .context("failed to get html tag from dom")
    }
}

pub trait QuerySelectorMutliple<'a> {
    fn query_selector_mutliple<'b, S, I>(
        &'b self,
        parser: &'b Parser<'a>,
        selectors: I,
    ) -> Result<&'b HTMLTag<'a>>
    where
        S: AsRef<str> + 'b,
        I: Iterator<Item = S>;
}

impl<'a> QuerySelectorMutliple<'a> for HTMLTag<'a> {
    fn query_selector_mutliple<'b, S, I>(
        &'b self,
        parser: &'b Parser<'a>,
        mut selectors: I,
    ) -> Result<&'b HTMLTag<'a>>
    where
        S: AsRef<str> + 'b,
        I: Iterator<Item = S>,
    {
        selectors.try_fold(self, |current, selector| -> Result<_> {
            let selector = selector.as_ref();

            let mut query = current
                .query_selector(parser, selector)
                .with_context(|| format!("failed to query selctor: '{selector}'"))?;

            query
                .next()
                .and_then(|node| node.get(parser))
                .and_then(|node| node.as_tag())
                .with_context(|| format!("failed to get node for selector: {selector:?}"))
        })
    }
}

//
//
//pub type QueryMap<'a> = HashMap<Cow<'a, str>, Cow<'a, str>>;
//
//pub fn query_hash_map_to_str(map: &QueryMap<'_>) -> String {
//    let mut query = String::new();
//
//    let len = map.len();
//    for (i, (key, value)) in map.iter().enumerate() {
//        query.push_str(key);
//        query.push('=');
//        query.push_str(value);
//        if i != len - 1 {
//            query.push('&');
//        }
//    }
//
//    query
//}
//
//pub fn get_query_hash_map<'a>(uri: &'a hyper::Uri) -> Option<QueryMap<'a>> {
//    let mut map = QueryMap::new();
//
//    for param in uri.query()?.split('&') {
//        let (key, value) = param.split_once('=')?;
//        map.insert(Cow::Borrowed(key), Cow::Borrowed(value));
//    }
//
//    Some(map)
//}
