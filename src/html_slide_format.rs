use ctype::isspace;
use nodes::{AstNode, ListType, NodeCode, NodeValue, TableAlignment};
use parser::ComrakOptions;
use regex::Regex;
use scanners;

use std::borrow::Cow;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::io::{self, Write};
use std::str::{self, FromStr};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct SlideHtmlDom {
    front: SlideSectionHtmlDom,
    content: Vec<SlideSectionHtmlDom>,
    format_level: u8,       // 0:cover  -1 footer 标记format时 当前在那一页
    format_content: String, // 当前页面的内容
    format_meta: HashMap<String, String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct SlideSectionHtmlDom {
    meta: HashMap<String, String>,
    content: String,
}

impl SlideHtmlDom {
    pub fn new() -> Self {
        SlideHtmlDom {
            front: SlideSectionHtmlDom {
                meta: HashMap::new(),
                content: String::new(),
            },

            content: Vec::new(),
            format_level: 0,
            format_content: String::new(),
            format_meta: HashMap::new(),
        }
    }
}

impl SlideSectionHtmlDom {
    pub fn new() -> Self {
        SlideSectionHtmlDom {
            meta: HashMap::new(),
            content: String::new(),
        }
    }
}

/// Formats an AST as HTML, modified by the given options.
pub fn format_document_slide<'a>(
    root: &'a AstNode<'a>,
    options: &ComrakOptions,
    output: &mut dyn Write,
) -> io::Result<()> {
    println!("format_document_slide");

    let mut writer = WriteWithLast {
        output,
        last_was_lf: Cell::new(true),
    };
    let mut jsonDom = SlideHtmlDom::new();

    let mut f = HtmlSlideFormatter::new(options, &mut writer);
    f.format(root, &mut jsonDom, false)?;
    f.setupSlideDomContent(root, &mut jsonDom);

    // if f.footnote_ix > 0 {
    //     f.output.write_all(b"</ol>\n</section>\n")?;
    // }

    let serialized = serde_json::to_string(&jsonDom).unwrap();
    println!("serialized = {}", serialized);
    Ok(())
}

pub struct WriteWithLast<'w> {
    output: &'w mut dyn Write,
    pub last_was_lf: Cell<bool>,
}

impl<'w> Write for WriteWithLast<'w> {
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let l = buf.len();
        if l > 0 {
            self.last_was_lf.set(buf[l - 1] == 10);
        }
        self.output.write(buf)
    }
}

#[rustfmt::skip]
const NEEDS_ESCAPED : [bool; 256] = [
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, true,  false, false, false, true,  false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, true, false, true, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
    false, false, false, false, false, false, false, false,
];

fn tagfilter(literal: &[u8]) -> bool {
    lazy_static! {
        static ref TAGFILTER_BLACKLIST: [&'static str; 9] = [
            "title",
            "textarea",
            "style",
            "xmp",
            "iframe",
            "noembed",
            "noframes",
            "script",
            "plaintext"
        ];
    }

    if literal.len() < 3 || literal[0] != b'<' {
        return false;
    }

    let mut i = 1;
    if literal[i] == b'/' {
        i += 1;
    }

    for t in TAGFILTER_BLACKLIST.iter() {
        if unsafe { String::from_utf8_unchecked(literal[i..].to_vec()) }
            .to_lowercase()
            .starts_with(t)
        {
            let j = i + t.len();
            return isspace(literal[j])
                || literal[j] == b'>'
                || (literal[j] == b'/' && literal.len() >= j + 2 && literal[j + 1] == b'>');
        }
    }

    false
}

fn tagfilter_block(input: &[u8], o: &mut dyn Write) -> io::Result<()> {
    let size = input.len();
    let mut i = 0;

    while i < size {
        let org = i;
        while i < size && input[i] != b'<' {
            i += 1;
        }

        if i > org {
            o.write_all(&input[org..i])?;
        }

        if i >= size {
            break;
        }

        if tagfilter(&input[i..]) {
            o.write_all(b"&lt;")?;
        } else {
            o.write_all(b"<")?;
        }

        i += 1;
    }

    Ok(())
}

fn dangerous_url(input: &[u8]) -> bool {
    scanners::dangerous_url(input).is_some()
}

#[derive(Debug, Default)]
pub struct Anchorizer(HashSet<String>);

impl Anchorizer {
    /// Construct a new anchorizer.
    pub fn new() -> Self {
        Anchorizer(HashSet::new())
    }

    /// Returns a String that has been converted into an anchor using the
    /// GFM algorithm, which involves changing spaces to dashes, removing
    /// problem characters and, if needed, adding a suffix to make the
    /// resultant anchor unique.
    ///
    /// ```
    /// use comrak::Anchorizer;
    ///
    /// let mut anchorizer = Anchorizer::new();
    ///
    /// let source = "Ticks aren't in";
    ///
    /// assert_eq!("ticks-arent-in".to_string(), anchorizer.anchorize(source.to_string()));
    /// ```
    pub fn anchorize(&mut self, header: String) -> String {
        lazy_static! {
            static ref REJECTED_CHARS: Regex = Regex::new(r"[^\p{L}\p{M}\p{N}\p{Pc} -]").unwrap();
        }

        let mut id = header;
        id = id.to_lowercase();
        id = REJECTED_CHARS.replace_all(&id, "").to_string();
        id = id.replace(' ', "-");

        let mut uniq = 0;
        id = loop {
            let anchor = if uniq == 0 {
                Cow::from(&*id)
            } else {
                Cow::from(format!("{}-{}", &id, uniq))
            };

            if !self.0.contains(&*anchor) {
                break anchor.to_string();
            }

            uniq += 1;
        };
        self.0.insert(id.clone());
        id
    }
}

struct HtmlSlideFormatter<'o> {
    output: &'o mut WriteWithLast<'o>,
    options: &'o ComrakOptions,
    anchorizer: Anchorizer,
    footnote_ix: u32,
    written_footnote_ix: u32,
    last_is_effect: bool,
}

impl<'o> HtmlSlideFormatter<'o> {
    fn new(options: &'o ComrakOptions, output: &'o mut WriteWithLast<'o>) -> Self {
        HtmlSlideFormatter {
            options,
            output,
            anchorizer: Anchorizer::new(),
            footnote_ix: 0,
            written_footnote_ix: 0,
            last_is_effect: false,
        }
    }

    fn cr(&mut self) -> io::Result<()> {
        if !self.output.last_was_lf.get() {
            self.output.write_all(b"\n")?;
        }
        Ok(())
    }

    fn escape(&mut self, buffer: &[u8]) -> io::Result<()> {
        let mut offset = 0;
        for (i, &byte) in buffer.iter().enumerate() {
            if NEEDS_ESCAPED[byte as usize] {
                let esc: &[u8] = match byte {
                    b'"' => b"&quot;",
                    b'&' => b"&amp;",
                    b'<' => b"&lt;",
                    b'>' => b"&gt;",
                    _ => unreachable!(),
                };
                self.output.write_all(&buffer[offset..i])?;
                self.output.write_all(esc)?;
                offset = i + 1;
            }
        }
        self.output.write_all(&buffer[offset..])?;
        Ok(())
    }

    fn escape_href(&mut self, buffer: &[u8]) -> io::Result<()> {
        lazy_static! {
            static ref HREF_SAFE: [bool; 256] = {
                let mut a = [false; 256];
                for &c in b"-_.+!*(),%#@?=;:/,+$~abcdefghijklmnopqrstuvwxyz".iter() {
                    a[c as usize] = true;
                }
                for &c in b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789".iter() {
                    a[c as usize] = true;
                }
                a
            };
        }

        let size = buffer.len();
        let mut i = 0;

        while i < size {
            let org = i;
            while i < size && HREF_SAFE[buffer[i] as usize] {
                i += 1;
            }

            if i > org {
                self.output.write_all(&buffer[org..i])?;
            }

            if i >= size {
                break;
            }

            match buffer[i] as char {
                '&' => {
                    self.output.write_all(b"&amp;")?;
                }
                '\'' => {
                    self.output.write_all(b"&#x27;")?;
                }
                _ => write!(self.output, "%{:02X}", buffer[i])?,
            }

            i += 1;
        }

        Ok(())
    }

    fn format<'a>(
        &mut self,
        node: &'a AstNode<'a>,
        jsonDom: &mut SlideHtmlDom,
        plain: bool,
    ) -> io::Result<()> {
        // Traverse the AST iteratively using a work stack, with pre- and
        // post-child-traversal phases. During pre-order traversal render the
        // opening tags, then push the node back onto the stack for the
        // post-order traversal phase, then push the children in reverse order
        // onto the stack and begin rendering first child.

        enum Phase {
            Pre,
            Post,
        }

        let mut stack = vec![(node, plain, Phase::Pre)];

        while let Some((node, plain, phase)) = stack.pop() {
            match phase {
                Phase::Pre => {
                    let new_plain;
                    if plain {
                        match node.data.borrow().value {
                            NodeValue::Text(ref literal)
                            | NodeValue::Code(NodeCode { ref literal, .. })
                            | NodeValue::HtmlInline(ref literal) => {
                                self.escape(literal)?;
                            }
                            NodeValue::LineBreak | NodeValue::SoftBreak => {
                                self.output.write_all(b" ")?;
                            }
                            _ => (),
                        }
                        new_plain = plain;
                    } else {
                        stack.push((node, false, Phase::Post));
                        new_plain = self.format_node(node, jsonDom, true)?;
                    }

                    for ch in node.reverse_children() {
                        stack.push((ch, new_plain, Phase::Pre));
                    }
                }
                Phase::Post => {
                    debug_assert!(!plain);
                    self.format_node(node, jsonDom, false)?;
                }
            }
        }

        Ok(())
    }

    fn collect_text<'a>(&self, node: &'a AstNode<'a>, output: &mut Vec<u8>) {
        match node.data.borrow().value {
            NodeValue::Text(ref literal) | NodeValue::Code(NodeCode { ref literal, .. }) => {
                output.extend_from_slice(literal)
            }
            NodeValue::LineBreak | NodeValue::SoftBreak => output.push(b' '),
            _ => {
                for n in node.children() {
                    self.collect_text(n, output);
                }
            }
        }
    }

    fn setupSlideDomContent<'a>(&mut self, node: &'a AstNode<'a>, jsonDom: &mut SlideHtmlDom) {
        if jsonDom.format_level == 0 {
            return;
        }

        if jsonDom.format_level == 1 {
            jsonDom.front.content = jsonDom.format_content.clone();
            jsonDom.front.meta = jsonDom.format_meta.clone();
        } else if jsonDom.format_level > 1 {
            let mut sectionDom = SlideSectionHtmlDom::new();
            sectionDom.content = jsonDom.format_content.clone();
            sectionDom.meta = jsonDom.format_meta.clone();
            jsonDom.content.push(sectionDom);
        }

        jsonDom.format_meta.clear();
        jsonDom.format_content.clear();
    }
    fn format_node<'a>(
        &mut self,
        node: &'a AstNode<'a>,
        jsonDom: &mut SlideHtmlDom,
        entering: bool,
    ) -> io::Result<bool> {
        match node.data.borrow().value {
            NodeValue::Document => (),
            NodeValue::FrontMatter(_) => (),
            NodeValue::BlockQuote => {
                if entering {
                    self.cr()?;
                    self.output.write_all(b"<blockquote>\n")?;
                } else {
                    self.cr()?;
                    self.output.write_all(b"</blockquote>\n")?;
                }
            }
            NodeValue::List(ref nl) => {
                if entering {
                    self.cr()?;
                    if nl.list_type == ListType::Bullet {
                        self.output.write_all(b"<ul>\n")?;
                    } else if nl.start == 1 {
                        self.output.write_all(b"<ol>\n")?;
                    } else {
                        writeln!(self.output, "<ol start=\"{}\">", nl.start)?;
                    }
                } else if nl.list_type == ListType::Bullet {
                    self.output.write_all(b"</ul>\n")?;
                } else {
                    self.output.write_all(b"</ol>\n")?;
                }
            }
            NodeValue::Item(..) => {
                if entering {
                    self.cr()?;
                    self.output.write_all(b"<li>")?;
                } else {
                    self.output.write_all(b"</li>\n")?;
                }
            }
            NodeValue::DescriptionList => {
                if entering {
                    jsonDom.format_content = format!("{}{}", jsonDom.format_content, "\n");
                    self.cr()?;
                    self.output.write_all(b"<dl>")?;
                } else {
                    self.output.write_all(b"</dl>\n")?;
                }
            }
            NodeValue::DescriptionItem(..) => (),
            NodeValue::DescriptionTerm => {
                if entering {
                    // jsonDom.format_content = format!("{}{}", jsonDom.format_content, "\n");
                    self.output.write_all(b"<dt>")?;
                } else {
                    self.output.write_all(b"</dt>\n")?;
                }
            }
            NodeValue::DescriptionDetails => {
                if entering {
                    // jsonDom.format_content = format!("{}{}", jsonDom.format_content, "\n");
                    self.output.write_all(b"<dd>")?;
                } else {
                    self.output.write_all(b"</dd>\n")?;
                }
            }
            NodeValue::SlideMetaDataBlock(ref smd) => {
                if entering {
                    let mut meta: HashMap<String, String> = HashMap::new();

                    for kv in &smd.metadatas {
                        meta.insert(
                            String::from_utf8_lossy(&kv.key).to_string(),
                            String::from_utf8_lossy(&kv.value).to_string(),
                        );
                    }

                    jsonDom.format_meta = meta.clone();
                }
            }
            NodeValue::Effect(ref effect) => {
                if entering {
                    self.last_is_effect = true;
                    self.cr()?;
                    let effcontent = String::from_utf8_lossy(&effect.literal);
                    writeln!(self.output, "<effect {}>", effcontent)?;
                    // self.output.write_all(b"\n</effect>\n");
                } else {
                    // self.output.write_all(b"</effect>\n")?;
                }
            }
            NodeValue::KV(ref _kv) => {}
            NodeValue::Heading(ref nch) => {
                if entering {
                    self.setupSlideDomContent(node, jsonDom);

                    if nch.level == 1 {
                        jsonDom.format_content = String::new();
                        jsonDom.format_content = format!("{}", "# ");
                    } else if nch.level == 2 {
                        jsonDom.format_content = format!("{}", "## ");
                    }

                    jsonDom.format_level += 1;

                    if self.last_is_effect {
                        self.last_is_effect = false;
                        self.output.write_all(b"\n</effect>\n");
                    }

                    self.cr()?;
                    write!(self.output, "<h{}>", nch.level)?;

                    if let Some(ref prefix) = self.options.extension.header_ids {
                        let mut text_content = Vec::with_capacity(20);
                        self.collect_text(node, &mut text_content);

                        let mut id = String::from_utf8(text_content).unwrap();
                        id = self.anchorizer.anchorize(id);
                        write!(
                            self.output,
                            "<a href=\"#{}\" aria-hidden=\"true\" class=\"anchor\" id=\"{}{}\"></a>",
                            id,
                            prefix,
                            id
                        )?;
                    }
                } else {
                    writeln!(self.output, "</h{}>tttt", nch.level)?;
                }
            }

            NodeValue::CodeBlock(ref ncb) => {
                if entering {
                    self.cr()?;

                    if ncb.info.is_empty() {
                        self.output.write_all(b"<pre><code>")?;
                    } else {
                        let mut first_tag = 0;
                        while first_tag < ncb.info.len() && !isspace(ncb.info[first_tag]) {
                            first_tag += 1;
                        }

                        if self.options.render.github_pre_lang {
                            self.output.write_all(b"<pre lang=\"")?;
                            self.escape(&ncb.info[..first_tag])?;
                            self.output.write_all(b"\"><code>")?;
                        } else {
                            self.output.write_all(b"<pre><code class=\"language-")?;
                            self.escape(&ncb.info[..first_tag])?;
                            self.output.write_all(b"\">")?;
                        }
                    }
                    self.escape(&ncb.literal)?;
                    self.output.write_all(b"</code></pre>\n")?;
                }
            }
            NodeValue::HtmlBlock(ref nhb) => {
                if entering {
                    self.cr()?;
                    if self.options.render.escape {
                        self.escape(&nhb.literal)?;
                    } else if !self.options.render.unsafe_ {
                        self.output.write_all(b"<!-- raw HTML omitted -->")?;
                    } else if self.options.extension.tagfilter {
                        tagfilter_block(&nhb.literal, &mut self.output)?;
                    } else {
                        self.output.write_all(&nhb.literal)?;
                    }
                    self.cr()?;
                }
            }
            NodeValue::ThematicBreak => {
                if entering {
                    self.cr()?;
                    self.output.write_all(b"<hr />\n")?;
                }
            }
            NodeValue::Text(ref literal) => {
                if entering {
                    self.escape(literal)?;
                } else {
                    // println!("{:?}", String::from_utf8_lossy(literal));
                    // println!("{:?}", node.parent());
                    match node.parent() {
                        Some(parent) => match parent.data.borrow().value {
                            NodeValue::Link(..) => {
                                jsonDom.format_content = format!(
                                    "{}[{}]",
                                    jsonDom.format_content,
                                    String::from_utf8_lossy(literal)
                                );
                            }
                            _ => match parent.parent() {
                                Some(parent) => match parent.data.borrow().value {
                                    NodeValue::Item(..) => {
                                        jsonDom.format_content = format!(
                                            "{}- {}",
                                            jsonDom.format_content,
                                            String::from_utf8_lossy(literal)
                                        );
                                    }
                                    _ => {
                                        jsonDom.format_content = format!(
                                            "{}{}",
                                            jsonDom.format_content,
                                            String::from_utf8_lossy(literal)
                                        );
                                    }
                                },
                                None => {
                                    jsonDom.format_content = format!(
                                        "{}{}",
                                        jsonDom.format_content,
                                        String::from_utf8_lossy(literal)
                                    );
                                }
                            },
                        },
                        None => {
                            jsonDom.format_content = format!(
                                "{}{}",
                                jsonDom.format_content,
                                String::from_utf8_lossy(literal)
                            );
                        }
                    }
                }
            }
            NodeValue::Paragraph => {
                if entering {
                    jsonDom.format_content = format!("{}{}", jsonDom.format_content, "\n");
                }

                if self.last_is_effect {
                    self.last_is_effect = false;
                    self.output.write_all(b"\n</effect>\n");
                }

                let tight = match node
                    .parent()
                    .and_then(|n| n.parent())
                    .map(|n| n.data.borrow().value.clone())
                {
                    Some(NodeValue::List(nl)) => nl.tight,
                    _ => false,
                };

                let tight = tight
                    || matches!(
                        node.parent().map(|n| n.data.borrow().value.clone()),
                        Some(NodeValue::DescriptionTerm)
                    );

                if !tight {
                    if entering {
                        self.cr()?;
                        self.output.write_all(b"<p>")?;
                    } else {
                        if matches!(
                            node.parent().unwrap().data.borrow().value,
                            NodeValue::FootnoteDefinition(..)
                        ) && node.next_sibling().is_none()
                        {
                            self.output.write_all(b" ")?;
                            self.put_footnote_backref()?;
                        }
                        self.output.write_all(b"</p>\n")?;
                    }
                }
            }

            NodeValue::LineBreak => {
                if entering {
                    jsonDom.format_content = format!("{}{}", jsonDom.format_content, "\n");
                    self.output.write_all(b"<br />\n")?;
                }
            }
            NodeValue::SoftBreak => {
                if entering {
                    jsonDom.format_content = format!("{}{}", jsonDom.format_content, "\n");

                    if self.options.render.hardbreaks {
                        self.output.write_all(b"<br />\n")?;
                    } else {
                        self.output.write_all(b"\n")?;
                    }
                }
            }
            NodeValue::Code(NodeCode { ref literal, .. }) => {
                if entering {
                    self.output.write_all(b"<code>")?;
                    self.escape(literal)?;
                    self.output.write_all(b"</code>")?;
                }
            }
            NodeValue::HtmlInline(ref literal) => {
                if entering {
                    if self.options.render.escape {
                        self.escape(&literal)?;
                    } else if !self.options.render.unsafe_ {
                        jsonDom.format_content = format!(
                            "{} {}",
                            jsonDom.format_content,
                            String::from_utf8_lossy(literal)
                        );

                        self.output.write_all(b"<!-- raw HTML omitted -->")?;
                    } else if self.options.extension.tagfilter && tagfilter(literal) {
                        self.output.write_all(b"&lt;")?;
                        self.output.write_all(&literal[1..])?;
                    } else {
                        self.output.write_all(literal)?;
                    }
                }
            }
            NodeValue::Strong => {
                if entering {
                    jsonDom.format_content = format!("{}{}", jsonDom.format_content, "\n");
                    self.output.write_all(b"<strong>")?;
                } else {
                    self.output.write_all(b"</strong>")?;
                }
            }
            NodeValue::Emph => {
                if entering {
                    self.output.write_all(b"<em>")?;
                } else {
                    self.output.write_all(b"</em>")?;
                }
            }
            NodeValue::Strikethrough => {
                if entering {
                    self.output.write_all(b"<del>")?;
                } else {
                    self.output.write_all(b"</del>")?;
                }
            }
            NodeValue::Superscript => {
                if entering {
                    self.output.write_all(b"<sup>")?;
                } else {
                    self.output.write_all(b"</sup>")?;
                }
            }
            NodeValue::Link(ref nl) => {
                if entering {
                    self.output.write_all(b"<a href=\"")?;
                    if self.options.render.unsafe_ || !dangerous_url(&nl.url) {
                        self.escape_href(&nl.url)?;
                    }
                    if !nl.title.is_empty() {
                        self.output.write_all(b"\" title=\"")?;
                        self.escape(&nl.title)?;
                    }
                    self.output.write_all(b"\">")?;
                } else {
                    self.output.write_all(b"</a>")?;

                    jsonDom.format_content = format!("{}{}", jsonDom.format_content, "\n");
                    if !nl.title.is_empty() {
                        jsonDom.format_content = format!(
                            "{}[{}]",
                            jsonDom.format_content,
                            String::from_utf8_lossy(&nl.title)
                        );
                    }

                    if self.options.render.unsafe_ || !dangerous_url(&nl.url) {
                        jsonDom.format_content = format!(
                            "{}({})",
                            jsonDom.format_content,
                            String::from_utf8_lossy(&nl.url)
                        );
                    }
                }
            }
            NodeValue::Image(ref nl) => {
                if entering {
                    self.output.write_all(b"<img src=\"")?;
                    if self.options.render.unsafe_ || !dangerous_url(&nl.url) {
                        self.escape_href(&nl.url)?;
                    }
                    self.output.write_all(b"\" alt=\"")?;
                    return Ok(true);
                } else {
                    if !nl.title.is_empty() {
                        self.output.write_all(b"\" title=\"")?;
                        self.escape(&nl.title)?;
                    }
                    self.output.write_all(b"\" />")?;

                    match node.first_child() {
                        Some(child) => match child.data.borrow().value {
                            NodeValue::Text(ref text) => {
                                jsonDom.format_content = format!(
                                    "{}![{}]",
                                    jsonDom.format_content,
                                    String::from_utf8_lossy(text)
                                );
                            }

                            _ => {
                                jsonDom.format_content = format!("{}![]", jsonDom.format_content,);
                            }
                        },
                        None => {
                            jsonDom.format_content = format!("{}![]", jsonDom.format_content,);
                        }
                    }

                    println!("image: {:?}", nl);
                    // if !nl.title.is_empty() {
                    //     jsonDom.format_content = format!(
                    //         "{}[{}]",
                    //         jsonDom.format_content,
                    //         String::from_utf8_lossy(&nl.title)
                    //     );
                    // }

                    if self.options.render.unsafe_ || !dangerous_url(&nl.url) {
                        jsonDom.format_content = format!(
                            "{}({})",
                            jsonDom.format_content,
                            String::from_utf8_lossy(&nl.url)
                        );
                    }
                }
            }
            NodeValue::Table(..) => {
                if entering {
                    self.cr()?;
                    self.output.write_all(b"<table>\n")?;
                } else {
                    if !node
                        .last_child()
                        .unwrap()
                        .same_node(node.first_child().unwrap())
                    {
                        self.cr()?;
                        self.output.write_all(b"</tbody>\n")?;
                    }
                    self.cr()?;
                    self.output.write_all(b"</table>\n")?;
                }
            }
            NodeValue::TableRow(header) => {
                if entering {
                    self.cr()?;
                    if header {
                        self.output.write_all(b"<thead>\n")?;
                    } else if let Some(n) = node.previous_sibling() {
                        if let NodeValue::TableRow(true) = n.data.borrow().value {
                            self.output.write_all(b"<tbody>\n")?;
                        }
                    }
                    self.output.write_all(b"<tr>")?;
                } else {
                    self.cr()?;
                    self.output.write_all(b"</tr>")?;
                    if header {
                        self.cr()?;
                        self.output.write_all(b"</thead>")?;
                    }
                }
            }
            NodeValue::TableCell => {
                let row = &node.parent().unwrap().data.borrow().value;
                let in_header = match *row {
                    NodeValue::TableRow(header) => header,
                    _ => panic!(),
                };

                let table = &node.parent().unwrap().parent().unwrap().data.borrow().value;
                let alignments = match *table {
                    NodeValue::Table(ref alignments) => alignments,
                    _ => panic!(),
                };

                if entering {
                    self.cr()?;
                    if in_header {
                        self.output.write_all(b"<th")?;
                    } else {
                        self.output.write_all(b"<td")?;
                    }

                    let mut start = node.parent().unwrap().first_child().unwrap();
                    let mut i = 0;
                    while !start.same_node(node) {
                        i += 1;
                        start = start.next_sibling().unwrap();
                    }

                    match alignments[i] {
                        TableAlignment::Left => {
                            self.output.write_all(b" align=\"left\"")?;
                        }
                        TableAlignment::Right => {
                            self.output.write_all(b" align=\"right\"")?;
                        }
                        TableAlignment::Center => {
                            self.output.write_all(b" align=\"center\"")?;
                        }
                        TableAlignment::None => (),
                    }

                    self.output.write_all(b">")?;
                } else if in_header {
                    self.output.write_all(b"</th>")?;
                } else {
                    self.output.write_all(b"</td>")?;
                }
            }
            NodeValue::FootnoteDefinition(_) => {
                if entering {
                    jsonDom.format_content = format!("{}{}", jsonDom.format_content, "\n");
                    if self.footnote_ix == 0 {
                        self.output
                            .write_all(b"<section class=\"footnotes\">\n<ol>\n")?;
                    }
                    self.footnote_ix += 1;
                    writeln!(self.output, "<li id=\"fn{}\">", self.footnote_ix)?;
                } else {
                    if self.put_footnote_backref()? {
                        self.output.write_all(b"\n")?;
                    }
                    self.output.write_all(b"</li>\n")?;
                }
            }
            NodeValue::FootnoteReference(ref r) => {
                if entering {
                    let r = str::from_utf8(r).unwrap();
                    write!(
                        self.output,
                        "<sup class=\"footnote-ref\"><a href=\"#fn{}\" id=\"fnref{}\">{}</a></sup>",
                        r, r, r
                    )?;
                }
            }
            NodeValue::TaskItem(checked) => {
                if entering {
                    if checked {
                        self.output.write_all(
                            b"<input type=\"checkbox\" disabled=\"\" checked=\"\" /> ",
                        )?;
                    } else {
                        self.output
                            .write_all(b"<input type=\"checkbox\" disabled=\"\" /> ")?;
                    }
                }
            }
        }
        Ok(false)
    }

    fn format_node_v2<'a>(&mut self, node: &'a AstNode<'a>, entering: bool) -> io::Result<bool> {
        match node.data.borrow().value {
            NodeValue::Document => (),
            NodeValue::FrontMatter(_) => (),
            NodeValue::BlockQuote => {
                if entering {
                    self.cr()?;
                    self.output.write_all(b"<blockquote>\n")?;
                } else {
                    self.cr()?;
                    self.output.write_all(b"</blockquote>\n")?;
                }
            }
            NodeValue::List(ref nl) => {
                if entering {
                    self.cr()?;
                    if nl.list_type == ListType::Bullet {
                        self.output.write_all(b"<ul>\n")?;
                    } else if nl.start == 1 {
                        self.output.write_all(b"<ol>\n")?;
                    } else {
                        writeln!(self.output, "<ol start=\"{}\">", nl.start)?;
                    }
                } else if nl.list_type == ListType::Bullet {
                    self.output.write_all(b"</ul>\n")?;
                } else {
                    self.output.write_all(b"</ol>\n")?;
                }
            }
            NodeValue::Item(..) => {
                if entering {
                    self.cr()?;
                    self.output.write_all(b"<li>")?;
                } else {
                    self.output.write_all(b"</li>\n")?;
                }
            }
            NodeValue::DescriptionList => {
                if entering {
                    self.cr()?;
                    self.output.write_all(b"<dl>")?;
                } else {
                    self.output.write_all(b"</dl>\n")?;
                }
            }
            NodeValue::DescriptionItem(..) => (),
            NodeValue::DescriptionTerm => {
                if entering {
                    self.output.write_all(b"<dt>")?;
                } else {
                    self.output.write_all(b"</dt>\n")?;
                }
            }
            NodeValue::DescriptionDetails => {
                if entering {
                    self.output.write_all(b"<dd>")?;
                } else {
                    self.output.write_all(b"</dd>\n")?;
                }
            }
            NodeValue::Heading(ref nch) => {
                if entering {
                    if self.last_is_effect {
                        self.last_is_effect = false;
                        self.output.write_all(b"\n</effect>\n");
                    }

                    self.cr()?;
                    write!(self.output, "<h{}>", nch.level)?;

                    if let Some(ref prefix) = self.options.extension.header_ids {
                        let mut text_content = Vec::with_capacity(20);
                        self.collect_text(node, &mut text_content);

                        let mut id = String::from_utf8(text_content).unwrap();
                        id = self.anchorizer.anchorize(id);
                        write!(
                            self.output,
                            "<a href=\"#{}\" aria-hidden=\"true\" class=\"anchor\" id=\"{}{}\"></a>",
                            id,
                            prefix,
                            id
                        )?;
                    }
                } else {
                    writeln!(self.output, "</h{}>tttt", nch.level)?;
                }
            }
            NodeValue::SlideMetaDataBlock(ref smd) => {}
            NodeValue::Effect(ref effect) => {
                if entering {
                    self.last_is_effect = true;
                    self.cr()?;
                    let effcontent = String::from_utf8_lossy(&effect.literal);
                    writeln!(self.output, "<effect {}>", effcontent)?;
                    // self.output.write_all(b"\n</effect>\n");
                } else {
                    // self.output.write_all(b"</effect>\n")?;
                }
            }
            NodeValue::KV(ref _kv) => {}
            NodeValue::CodeBlock(ref ncb) => {
                if entering {
                    self.cr()?;

                    if ncb.info.is_empty() {
                        self.output.write_all(b"<pre><code>")?;
                    } else {
                        let mut first_tag = 0;
                        while first_tag < ncb.info.len() && !isspace(ncb.info[first_tag]) {
                            first_tag += 1;
                        }

                        if self.options.render.github_pre_lang {
                            self.output.write_all(b"<pre lang=\"")?;
                            self.escape(&ncb.info[..first_tag])?;
                            self.output.write_all(b"\"><code>")?;
                        } else {
                            self.output.write_all(b"<pre><code class=\"language-")?;
                            self.escape(&ncb.info[..first_tag])?;
                            self.output.write_all(b"\">")?;
                        }
                    }
                    self.escape(&ncb.literal)?;
                    self.output.write_all(b"</code></pre>\n")?;
                }
            }
            NodeValue::HtmlBlock(ref nhb) => {
                if entering {
                    self.cr()?;
                    if self.options.render.escape {
                        self.escape(&nhb.literal)?;
                    } else if !self.options.render.unsafe_ {
                        self.output.write_all(b"<!-- raw HTML omitted -->")?;
                    } else if self.options.extension.tagfilter {
                        tagfilter_block(&nhb.literal, &mut self.output)?;
                    } else {
                        self.output.write_all(&nhb.literal)?;
                    }
                    self.cr()?;
                }
            }
            NodeValue::ThematicBreak => {
                if entering {
                    self.cr()?;
                    self.output.write_all(b"<hr />\n")?;
                }
            }
            NodeValue::Paragraph => {
                if self.last_is_effect {
                    self.last_is_effect = false;
                    self.output.write_all(b"\n</effect>\n");
                }

                let tight = match node
                    .parent()
                    .and_then(|n| n.parent())
                    .map(|n| n.data.borrow().value.clone())
                {
                    Some(NodeValue::List(nl)) => nl.tight,
                    _ => false,
                };

                let tight = tight
                    || matches!(
                        node.parent().map(|n| n.data.borrow().value.clone()),
                        Some(NodeValue::DescriptionTerm)
                    );

                if !tight {
                    if entering {
                        self.cr()?;
                        self.output.write_all(b"<p>")?;
                    } else {
                        if matches!(
                            node.parent().unwrap().data.borrow().value,
                            NodeValue::FootnoteDefinition(..)
                        ) && node.next_sibling().is_none()
                        {
                            self.output.write_all(b" ")?;
                            self.put_footnote_backref()?;
                        }
                        self.output.write_all(b"</p>\n")?;
                    }
                }
            }
            NodeValue::Text(ref literal) => {
                if entering {
                    self.escape(literal)?;
                }
            }
            NodeValue::LineBreak => {
                if entering {
                    self.output.write_all(b"<br />\n")?;
                }
            }
            NodeValue::SoftBreak => {
                if entering {
                    if self.options.render.hardbreaks {
                        self.output.write_all(b"<br />\n")?;
                    } else {
                        self.output.write_all(b"\n")?;
                    }
                }
            }
            NodeValue::Code(NodeCode { ref literal, .. }) => {
                if entering {
                    self.output.write_all(b"<code>")?;
                    self.escape(literal)?;
                    self.output.write_all(b"</code>")?;
                }
            }
            NodeValue::HtmlInline(ref literal) => {
                if entering {
                    if self.options.render.escape {
                        self.escape(&literal)?;
                    } else if !self.options.render.unsafe_ {
                        self.output.write_all(b"<!-- raw HTML omitted -->")?;
                    } else if self.options.extension.tagfilter && tagfilter(literal) {
                        self.output.write_all(b"&lt;")?;
                        self.output.write_all(&literal[1..])?;
                    } else {
                        self.output.write_all(literal)?;
                    }
                }
            }
            NodeValue::Strong => {
                if entering {
                    self.output.write_all(b"<strong>")?;
                } else {
                    self.output.write_all(b"</strong>")?;
                }
            }
            NodeValue::Emph => {
                if entering {
                    self.output.write_all(b"<em>")?;
                } else {
                    self.output.write_all(b"</em>")?;
                }
            }
            NodeValue::Strikethrough => {
                if entering {
                    self.output.write_all(b"<del>")?;
                } else {
                    self.output.write_all(b"</del>")?;
                }
            }
            NodeValue::Superscript => {
                if entering {
                    self.output.write_all(b"<sup>")?;
                } else {
                    self.output.write_all(b"</sup>")?;
                }
            }
            NodeValue::Link(ref nl) => {
                if entering {
                    self.output.write_all(b"<a href=\"")?;
                    if self.options.render.unsafe_ || !dangerous_url(&nl.url) {
                        self.escape_href(&nl.url)?;
                    }
                    if !nl.title.is_empty() {
                        self.output.write_all(b"\" title=\"")?;
                        self.escape(&nl.title)?;
                    }
                    self.output.write_all(b"\">")?;
                } else {
                    self.output.write_all(b"</a>")?;
                }
            }
            NodeValue::Image(ref nl) => {
                if entering {
                    self.output.write_all(b"<img src=\"")?;
                    if self.options.render.unsafe_ || !dangerous_url(&nl.url) {
                        self.escape_href(&nl.url)?;
                    }
                    self.output.write_all(b"\" alt=\"")?;
                    return Ok(true);
                } else {
                    if !nl.title.is_empty() {
                        self.output.write_all(b"\" title=\"")?;
                        self.escape(&nl.title)?;
                    }
                    self.output.write_all(b"\" />")?;
                }
            }
            NodeValue::Table(..) => {
                if entering {
                    self.cr()?;
                    self.output.write_all(b"<table>\n")?;
                } else {
                    if !node
                        .last_child()
                        .unwrap()
                        .same_node(node.first_child().unwrap())
                    {
                        self.cr()?;
                        self.output.write_all(b"</tbody>\n")?;
                    }
                    self.cr()?;
                    self.output.write_all(b"</table>\n")?;
                }
            }
            NodeValue::TableRow(header) => {
                if entering {
                    self.cr()?;
                    if header {
                        self.output.write_all(b"<thead>\n")?;
                    } else if let Some(n) = node.previous_sibling() {
                        if let NodeValue::TableRow(true) = n.data.borrow().value {
                            self.output.write_all(b"<tbody>\n")?;
                        }
                    }
                    self.output.write_all(b"<tr>")?;
                } else {
                    self.cr()?;
                    self.output.write_all(b"</tr>")?;
                    if header {
                        self.cr()?;
                        self.output.write_all(b"</thead>")?;
                    }
                }
            }
            NodeValue::TableCell => {
                let row = &node.parent().unwrap().data.borrow().value;
                let in_header = match *row {
                    NodeValue::TableRow(header) => header,
                    _ => panic!(),
                };

                let table = &node.parent().unwrap().parent().unwrap().data.borrow().value;
                let alignments = match *table {
                    NodeValue::Table(ref alignments) => alignments,
                    _ => panic!(),
                };

                if entering {
                    self.cr()?;
                    if in_header {
                        self.output.write_all(b"<th")?;
                    } else {
                        self.output.write_all(b"<td")?;
                    }

                    let mut start = node.parent().unwrap().first_child().unwrap();
                    let mut i = 0;
                    while !start.same_node(node) {
                        i += 1;
                        start = start.next_sibling().unwrap();
                    }

                    match alignments[i] {
                        TableAlignment::Left => {
                            self.output.write_all(b" align=\"left\"")?;
                        }
                        TableAlignment::Right => {
                            self.output.write_all(b" align=\"right\"")?;
                        }
                        TableAlignment::Center => {
                            self.output.write_all(b" align=\"center\"")?;
                        }
                        TableAlignment::None => (),
                    }

                    self.output.write_all(b">")?;
                } else if in_header {
                    self.output.write_all(b"</th>")?;
                } else {
                    self.output.write_all(b"</td>")?;
                }
            }
            NodeValue::FootnoteDefinition(_) => {
                if entering {
                    if self.footnote_ix == 0 {
                        self.output
                            .write_all(b"<section class=\"footnotes\">\n<ol>\n")?;
                    }
                    self.footnote_ix += 1;
                    writeln!(self.output, "<li id=\"fn{}\">", self.footnote_ix)?;
                } else {
                    if self.put_footnote_backref()? {
                        self.output.write_all(b"\n")?;
                    }
                    self.output.write_all(b"</li>\n")?;
                }
            }
            NodeValue::FootnoteReference(ref r) => {
                if entering {
                    let r = str::from_utf8(r).unwrap();
                    write!(
                        self.output,
                        "<sup class=\"footnote-ref\"><a href=\"#fn{}\" id=\"fnref{}\">{}</a></sup>",
                        r, r, r
                    )?;
                }
            }
            NodeValue::TaskItem(checked) => {
                if entering {
                    if checked {
                        self.output.write_all(
                            b"<input type=\"checkbox\" disabled=\"\" checked=\"\" /> ",
                        )?;
                    } else {
                        self.output
                            .write_all(b"<input type=\"checkbox\" disabled=\"\" /> ")?;
                    }
                }
            }
        }
        Ok(false)
    }

    fn put_footnote_backref(&mut self) -> io::Result<bool> {
        if self.written_footnote_ix >= self.footnote_ix {
            return Ok(false);
        }

        self.written_footnote_ix = self.footnote_ix;
        write!(
            self.output,
            "<a href=\"#fnref{}\" class=\"footnote-backref\">↩</a>",
            self.footnote_ix
        )?;
        Ok(true)
    }
}
