//! Pure textual helpers shared with the configuration manager.

use anyhow::Result;

/// Preserve an inherited animation's complete source, including comments,
/// shader strings, independent Glide values and arbitrary CSS color precision.
pub(crate) fn animation_blocks<'a>(source: &'a str, name: &str) -> Result<Vec<&'a str>> {
    let document = knuffel::parse_ast::<knuffel::span::Span>("config.kdl", source)
        .map_err(|error| anyhow::anyhow!("{error:?}"))?;
    Ok(document
        .nodes
        .iter()
        .filter(|node| &**node.node_name == "animations")
        .flat_map(|node| node.children())
        .filter(|node| &**node.node_name == name)
        .map(|node| {
            let span = node.span();
            &source[span.0..span.1]
        })
        .collect())
}
