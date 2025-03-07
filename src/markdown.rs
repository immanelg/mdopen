use pulldown_cmark::{html::push_html, CowStr, Event, Tag, TagEnd};

fn to_tag_anchor(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-')
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
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
                Event::Html(CowStr::from(format!(r##"<a id="{anchor}" class="anchor" href="#{anchor}"><span class="octicon octicon-link"></span></a>{text}"##)))
            } else {
                Event::Text(text)
            }
        }
        _ => event,
    });

    let mut html_output = String::new();
    push_html(&mut html_output, parser);

    html_output
}
