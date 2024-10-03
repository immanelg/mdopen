use pulldown_cmark::{BlockQuoteKind, CowStr, Event, Tag, TagEnd, html::push_html};

fn to_tag_anchor(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-')
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

fn blockquote_kind_to_css_class(kind: BlockQuoteKind) -> &'static str {
    match kind {
        BlockQuoteKind::Tip => "tip",
        BlockQuoteKind::Note => "note",
        BlockQuoteKind::Warning => "warning",
        BlockQuoteKind::Caution => "caution",
        BlockQuoteKind::Important => "important",
    }
}

pub fn to_html(md: &str) -> String {
    use pulldown_cmark::{Options, Parser};

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
                Event::Html(CowStr::from(format!(r##"<a id="{anchor}" class="anchor" href="#{anchor}">
<svg class="octicon octicon-link" viewBox="0 0 16 16" version="1.1" width="16" height="16" aria-hidden="true"><path d="m7.775 3.275 1.25-1.25a3.5 3.5 0 1 1 4.95 4.95l-2.5 2.5a3.5 3.5 0 0 1-4.95 0 .751.751 0 0 1 .018-1.042.751.751 0 0 1 1.042-.018 1.998 1.998 0 0 0 2.83 0l2.5-2.5a2.002 2.002 0 0 0-2.83-2.83l-1.25 1.25a.751.751 0 0 1-1.042-.018.751.751 0 0 1-.018-1.042Zm-4.69 9.64a1.998 1.998 0 0 0 2.83 0l1.25-1.25a.751.751 0 0 1 1.042.018.751.751 0 0 1 .018 1.042l-1.25 1.25a3.5 3.5 0 1 1-4.95-4.95l2.5-2.5a3.5 3.5 0 0 1 4.95 0 .751.751 0 0 1-.018 1.042.751.751 0 0 1-1.042.018 1.998 1.998 0 0 0-2.83 0l-2.5 2.5a1.998 1.998 0 0 0 0 2.83Z"></path></svg>
</a>{text}"##)))
            } else {
                Event::Text(text)
            }
        }
        Event::Start(Tag::BlockQuote(kind)) => {
            match kind {
                Some(kind) => {
                    let cls = blockquote_kind_to_css_class(kind);
                    Event::Html(CowStr::from(format!(r#"<blockquote class="markdown-alert-{cls}">"#)))
                }
               None => Event::Html(CowStr::from(format!(r#"<blockquote>"#)))
            }
        },
        Event::End(TagEnd::BlockQuote(_kind)) => {
            Event::Html(CowStr::from(r#"</blockquote>"#))
        },
        _ => event,
    });

    let mut html_output = String::new();
    push_html(&mut html_output, parser);

    return html_output;
}
