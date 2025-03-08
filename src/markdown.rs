use pulldown_cmark::TextMergeStream;
use pulldown_cmark::{html::push_html, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::iter::Iterator;
use std::sync::OnceLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::html::{
    append_highlighted_html_for_styled_line, start_highlighted_html_snippet, IncludeBackground,
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
    pub fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
        }
    }

    pub fn highlight(&self, code: &str, lang: Option<&str>) -> String {
        let theme = &self.theme_set.themes["base16-ocean.dark"];

        let syntax = lang
            .and_then(|l| self.syntax_set.find_syntax_by_token(l))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, theme);
        let (mut output, bg) = start_highlighted_html_snippet(theme);
        output.push_str("<code>");

        //if lang.is_empty() {
        //    output.push_str("<pre><code>")
        //} else {
        //    output.push_str("<pre><code class=\"language-");
        //    pulldown_cmark::escape_html(&mut self.writer, lang)?;
        //    output.push_str("\">")
        //}
        //
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

//pub(crate) struct DecoratedParser<'a> {
//    parser: pulldown_cmark::Parser<'a>,
//    syntax: SyntaxHighligher,
//    lang: Option<String>,
//    code: Option<Vec<pulldown_cmark::CowStr<'a>>>,
//    theme: &'a str,
//
//}
//
//impl<'a> DecoratedParser<'a> {
//    pub(crate) fn new(
//        parser: pulldown_cmark::Parser<'a>,
//        syntax: SyntaxHighligher,
//        theme: &'a str,
//    ) -> Self {
//        DecoratedParser {
//            parser,
//            syntax,
//            theme,
//            lang: None,
//            code: None,
//        }
//    }
//}
//
//impl<'a> Iterator for DecoratedParser<'a> {
//    type Item = Event<'a>;
//
//    fn next(&mut self) -> Option<Event<'a>> {
//        match self.parser.next() {
//            Some(Event::Text(text)) => {
//                if let Some(ref mut code) = self.code {
//                    code.push(text);
//                    Some(Event::Text(pulldown_cmark::CowStr::Borrowed("")))
//                } else {
//                    Some(Event::Text(text))
//                }
//            }
//            Some(Event::Start(Tag::CodeBlock(info))) => {
//                let tag = match info {
//                    pulldown_cmark::CodeBlockKind::Indented => "",
//                    pulldown_cmark::CodeBlockKind::Fenced(ref tag) => tag.as_ref(),
//                };
//                self.lang = tag.split(' ').map(|s| s.to_owned()).next();
//                self.code = Some(vec![]);
//                Some(Event::Text(pulldown_cmark::CowStr::Borrowed("")))
//            }
//            Some(Event::End(TagEnd::CodeBlock)) => {
//                let html = if let Some(code) = self.code.as_deref() {
//                    let code = code.iter().join("\n"); // itertools?
//                    self.syntax.format(&code, self.lang.as_deref(), self.theme)
//                } else {
//                    self.syntax.format("", self.lang.as_deref(), self.theme)
//                };
//                self.lang = None;
//                self.code = None;
//                Some(Event::Html(pulldown_cmark::CowStr::Boxed(html.into_boxed_str())))
//            }
//            item => item,
//        }
//    }
//}

fn map_highlighted_codeblocks<'a>(
    parser: impl Iterator<Item = Event<'a>>,
) -> impl Iterator<Item = Event<'a>> {
    static SYNTAX: OnceLock<SyntaxHighligher> = OnceLock::new();
    let syntax = SYNTAX.get_or_init(SyntaxHighligher::new);

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
