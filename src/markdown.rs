use pulldown_cmark::TextMergeStream;
use pulldown_cmark::{html::push_html, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::iter::Iterator;
use std::sync::OnceLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::html::{
    append_highlighted_html_for_styled_line, start_highlighted_html_snippet, ClassStyle, ClassedHTMLGenerator, IncludeBackground
};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

use crate::AppConfig;

fn to_tag_anchor(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-')
        .map(|c| if c == ' ' { '-' } else { c })
        .collect()
}

pub struct SyntaxHighligher {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl SyntaxHighligher {
    pub fn load() -> Self {
        let mut theme_set = ThemeSet::new(); // empty

        let github_dark = ThemeSet::load_from_reader(&mut std::io::Cursor::new(include_bytes!("./vendor/GitHub_Dark.tmTheme"))).unwrap();
        let github_light = ThemeSet::load_from_reader(&mut std::io::Cursor::new(include_bytes!("./vendor/GitHub_Light.tmTheme"))).unwrap();

        theme_set.themes.insert("github-dark".to_string(), github_dark);
        theme_set.themes.insert("github-light".to_string(), github_light);

        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set,
        }
    }

    pub fn highlight(&self, code: &str, lang: Option<&str>) -> String {
        //let syntax = lang
        //    .and_then(|l| self.syntax_set.find_syntax_by_token(l))
        //    .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());
        //
        //let mut output = String::with_capacity(64);
        //output.push_str("<pre><code>");
        //
        //let mut html_generator = ClassedHTMLGenerator::new_with_class_style(
        //    syntax, &self.syntax_set, ClassStyle::Spaced);
        //
        //for line in LinesWithEndings::from(code) {
        //    html_generator.parse_html_for_line_which_includes_newline(line).unwrap();
        //}
        //let inner = html_generator.finalize();
        //print!("{}", inner);
        //output.push_str(&inner);
        //output.push_str("</code></pre>");
        //output

        let theme = &self.theme_set.themes["github-dark"];

        let syntax = lang
            .and_then(|l| self.syntax_set.find_syntax_by_token(l))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, theme);
        let (mut output, bg) = start_highlighted_html_snippet(theme);
        output.push_str("<code>");

        for line in LinesWithEndings::from(code) {
            let regions = highlighter.highlight_line(line, &self.syntax_set).unwrap();
            append_highlighted_html_for_styled_line(
                &regions[..],
                IncludeBackground::IfDifferent(bg),
                &mut output,
            )
            .unwrap();
        }

        output.push_str("</code></pre>\n");
        output
    }
}

fn map_highlighted_codeblocks<'a>(
    parser: impl Iterator<Item = Event<'a>>,
) -> impl Iterator<Item = Event<'a>> {
    static SYNTAX: OnceLock<SyntaxHighligher> = OnceLock::new();
    let syntax = SYNTAX.get_or_init(SyntaxHighligher::load);

    let mut in_code_block = false;
    let mut lang = None;

    let parser = parser.map(move |event| match event {
        Event::Start(Tag::CodeBlock(kind)) => {
            in_code_block = true;
            let tag = match kind {
                CodeBlockKind::Indented => "",
                CodeBlockKind::Fenced(ref tag) => tag.as_ref(),
            };
            lang = tag.split(' ').map(|s| s.to_owned()).next();
            Event::Text(pulldown_cmark::CowStr::Borrowed(""))
        }

        Event::End(TagEnd::CodeBlock) => Event::Text(pulldown_cmark::CowStr::Borrowed("")),
        Event::Text(code) if in_code_block => {
            let html = syntax.highlight(code.as_ref(), lang.as_deref());
            in_code_block = false;
            lang = None;
            Event::Html(pulldown_cmark::CowStr::Boxed(html.into_boxed_str()))
        }
        _ => event,
    });
    parser
}
pub fn to_html(md: &str, config: &AppConfig) -> String {
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

    let parser: Box<dyn Iterator<Item = Event>> = if config.enable_syntax_highlight {
        Box::new(map_highlighted_codeblocks::<'_>(parser))
    } else {
        Box::new(parser)
    };

    let mut html_output = String::new();
    push_html(&mut html_output, parser);

    html_output
}
