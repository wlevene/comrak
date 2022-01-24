//! A 100% [CommonMark](http://commonmark.org/) and [GFM](https://github.github.com/gfm/)
//! compatible Markdown parser.  Source repository is at <https://github.com/kivikakk/comrak>.
//!
//! The design is based on [cmark](https://github.com/github/cmark), so familiarity with that will
//! help.
//!
//! You can use `comrak::markdown_to_html` directly:
//!
//! ```
//! use comrak::{markdown_to_html, ComrakOptions};
//! assert_eq!(markdown_to_html("Hello, **世界**!", &ComrakOptions::default()),
//!            "<p>Hello, <strong>世界</strong>!</p>\n");
//! ```
//!
//! Or you can parse the input into an AST yourself, manipulate it, and then use your desired
//! formatter:
//!
//! ```
//! extern crate comrak;
//! use comrak::{Arena, parse_document, format_html, ComrakOptions};
//! use comrak::nodes::{AstNode, NodeValue};
//!
//! # fn main() {
//! // The returned nodes are created in the supplied Arena, and are bound by its lifetime.
//! let arena = Arena::new();
//!
//! let root = parse_document(
//!     &arena,
//!     "This is my input.\n\n1. Also my input.\n2. Certainly my input.\n",
//!     &ComrakOptions::default());
//!
//! fn iter_nodes<'a, F>(node: &'a AstNode<'a>, f: &F)
//!     where F : Fn(&'a AstNode<'a>) {
//!     f(node);
//!     for c in node.children() {
//!         iter_nodes(c, f);
//!     }
//! }
//!
//! iter_nodes(root, &|node| {
//!     match &mut node.data.borrow_mut().value {
//!         &mut NodeValue::Text(ref mut text) => {
//!             let orig = std::mem::replace(text, vec![]);
//!             *text = String::from_utf8(orig).unwrap().replace("my", "your").as_bytes().to_vec();
//!         }
//!         _ => (),
//!     }
//! });
//!
//! let mut html = vec![];
//! format_html(root, &ComrakOptions::default(), &mut html).unwrap();
//!
//! assert_eq!(
//!     String::from_utf8(html).unwrap(),
//!     "<p>This is your input.</p>\n\
//!      <ol>\n\
//!      <li>Also your input.</li>\n\
//!      <li>Certainly your input.</li>\n\
//!      </ol>\n");
//! # }
//! ```

#![deny(
    missing_docs,
    missing_debug_implementations,
    missing_copy_implementations,
    trivial_casts,
    trivial_numeric_casts,
    unstable_features,
    unused_import_braces
)]
#![allow(unknown_lints, clippy::doc_markdown, cyclomatic_complexity)]

extern crate entities;
#[macro_use]
extern crate lazy_static;
extern crate pest;
#[macro_use]
extern crate pest_derive;
#[cfg(test)]
extern crate propfuzz;
extern crate regex;
#[cfg(feature = "benchmarks")]
extern crate test;
#[cfg(test)]
extern crate timebomb;
extern crate twoway;
extern crate typed_arena;
extern crate unicode_categories;

extern crate wasm_bindgen;
use wasm_bindgen::prelude::*;

extern crate serde;
use serde::{Deserialize, Serialize};

pub mod arena_tree;
mod cm;
mod ctype;
mod entity;
mod html;
mod html_slide_format;
pub mod nodes;
mod parser;
mod scanners;
mod strings;
#[cfg(test)]
mod tests;

pub use cm::format_document as format_commonmark;
pub use html::format_document as format_html;
pub use html::Anchorizer;
pub use html_slide_format::format_document_slide as format_slide;
pub use parser::{
    dump_node, parse_document, parse_document_with_broken_link_callback, ComrakExtensionOptions,
    ComrakOptions, ComrakParseOptions, ComrakRenderOptions,
};
pub use typed_arena::Arena;

/// Render Markdown to HTML.
///
/// See the documentation of the crate root for an example.
pub fn markdown_to_html(md: &str, options: &ComrakOptions) -> String {
    let arena = Arena::new();
    let root = parse_document(&arena, md, options);
    let mut s = Vec::new();
    format_html(root, options, &mut s).unwrap();
    String::from_utf8(s).unwrap()
}

/// Render Markdown to HTML For WebAssmbly.
///
/// See the documentation of the crate root for an example.
// #[wasm_bindgen]
// pub fn markdown_to_html_wasm_bindgen(md: &str) -> String {
//     let opt = comrakOpt();
//     let arena = Arena::new();
//     let root = parse_document(&arena, md, opt);
//     let mut s = Vec::new();
//     format_html(root, options, &mut s).unwrap();
//     String::from_utf8(s).unwrap()
// }

// #[wasm_bindgen(js_namespace = console)]
// pub fn log(s: &str);

// #[wasm_bindgen(js_namespace = console, js_name = log)]
// pub fn log_u32(a: u32);

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    pub fn log(s: &str);
    #[wasm_bindgen(js_namespace = console)]
    pub fn info(s: &str);
    #[wasm_bindgen(js_namespace = console)]
    pub fn warn(s: &str);
    #[wasm_bindgen(js_namespace = console)]
    pub fn error(s: &str);

    fn alert(s: &str);
}

/// Test For WebAssmbly 111.
///
/// See the documentation of the crate root for an example.
#[wasm_bindgen]
pub fn greet(name: &str) {
    alert(&format!("Hello, {}!", name));
}

/// Test For WebAssmbly.
///
/// See the documentation of the crate root for an example.
#[wasm_bindgen]
pub fn test_echo(a: i32) -> i32 {
    a + 1
}

fn comrak_opt() -> ComrakOptions {
    let opts = ComrakOptions {
        extension: ComrakExtensionOptions {
            strikethrough: true,
            tagfilter: true,
            table: true,
            autolink: true,
            tasklist: true,
            superscript: true,
            footnotes: true,
            description_lists: true,
            ..ComrakExtensionOptions::default()
        },
        render: ComrakRenderOptions {
            hardbreaks: true,
            ..ComrakRenderOptions::default()
        },
        ..ComrakOptions::default()
    };
    opts
}
