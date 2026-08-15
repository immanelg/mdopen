use pulldown_cmark::TextMergeStream;
use pulldown_cmark::{html::push_html, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::iter::Iterator;

use crate::app_config::AppConfig;

fn to_tag_anchor(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-')
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn render_mermaid_block(code: &str) -> String {
    format!(
        r#"<pre class="mermaid"><code class="language-mermaid">{}</code></pre>"#,
        escape_html(code)
    )
}

fn map_mermaid_codeblocks<'a>(
    parser: impl Iterator<Item = Event<'a>>,
) -> impl Iterator<Item = Event<'a>> {
    let mut in_code_block = false;
    let mut is_mermaid = false;

    parser.map(move |event| match event {
        Event::Start(Tag::CodeBlock(kind)) => {
            in_code_block = true;
            let kind = kind.clone();
            let tag = match &kind {
                CodeBlockKind::Indented => "",
                CodeBlockKind::Fenced(tag) => tag.as_ref(),
            };
            is_mermaid = tag
                .split(' ')
                .map(|s| s.to_ascii_lowercase())
                .next()
                .is_some_and(|lang| lang == "mermaid");
            if is_mermaid {
                Event::Text(pulldown_cmark::CowStr::Borrowed(""))
            } else {
                Event::Start(Tag::CodeBlock(kind))
            }
        }
        Event::End(TagEnd::CodeBlock) => {
            if is_mermaid {
                in_code_block = false;
                is_mermaid = false;
                Event::Text(pulldown_cmark::CowStr::Borrowed(""))
            } else {
                Event::End(TagEnd::CodeBlock)
            }
        }
        Event::Text(code) if in_code_block && is_mermaid => {
            in_code_block = false;
            is_mermaid = false;
            Event::Html(pulldown_cmark::CowStr::Boxed(
                render_mermaid_block(code.as_ref()).into_boxed_str(),
            ))
        }
        _ => event,
    })
}

pub fn to_html(md: &str, #[allow(unused)] config: &AppConfig) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);
    options.insert(Options::ENABLE_MATH);
    options.insert(Options::ENABLE_GFM);
    options.insert(Options::ENABLE_MATH);

    let parser = Parser::new_ext(md, options);
    let parser = TextMergeStream::new(parser);
    let parser = map_mermaid_codeblocks(parser);

    let mut inside_heading_level = false;

    let parser = parser.map(|event| match event {
        Event::Start(Tag::Heading { level, id, classes, attrs }) => {
            inside_heading_level = true;
            Event::Start(Tag::Heading { level, id, classes, attrs })
        }
        Event::End(TagEnd::Heading(level)) => {
            inside_heading_level = false;
            Event::End(TagEnd::Heading(level))
        }
        Event::Text(text) => {
            if inside_heading_level {
                let anchor = to_tag_anchor(&text);
                Event::Html(pulldown_cmark::CowStr::from(format!(r##"<a id="{anchor}" class="anchor" href="#{anchor}"><span class="octicon octicon-link"></span></a>{text}"##)))
            } else {
                Event::Text(text)
            }
        }
        _ => event,
    });

    #[cfg(feature = "syntax")]
    let parser: Box<dyn Iterator<Item = Event>> = if config.enable_syntax_highlight {
        Box::new(crate::syntax::map_highlighted_codeblocks::<'_>(parser))
    } else {
        Box::new(parser)
    };

    let mut html_output = String::new();
    push_html(&mut html_output, parser);

    html_output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_config::AppConfig;
    use crate::cli::Theme;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn test_config() -> AppConfig {
        AppConfig {
            addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            enable_reload: false,
            enable_latex: false,
            enable_syntax_highlight: true,
            theme: Theme::Auto,
        }
    }

    #[test]
    fn renders_mermaid_code_block_as_mermaid_node() {
        let html = to_html("```mermaid\ngraph TD;\n```", &test_config());
        assert!(html.contains(r#"class="mermaid""#));
        assert!(html.contains(r#"language-mermaid"#));
        assert!(html.contains("graph TD;"));
    }
}
