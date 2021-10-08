use std::{fmt::Debug, fs::File, io::Read};

use comrak::nodes::NodeKV;

use comrak::{
    format_html,
    nodes::{AstNode, NodeCode, NodeValue},
    parse_document, Arena, ComrakExtensionOptions, ComrakOptions, ComrakRenderOptions,
};

// Samples used in the README.  Wanna make sure they work as advertised.

extern crate comrak;

fn readfile(filename: &str) -> String {
    let mut f = File::open(filename).expect("file not found");
    let mut contents = String::new();
    f.read_to_string(&mut contents)
        .expect("something went wrong reading the file");
    contents
}

fn comrakOpt() -> ComrakOptions {
    let mut opts = ComrakOptions {
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

fn parseSMD() {
    let filename = "/Users/gangwang/root/code/github/comrak/examples/s.md";
    let md_content = readfile(filename);

    use comrak::nodes::{AstNode, NodeValue};
    use comrak::{dump_node, format_html, parse_document, Arena, ComrakOptions};

    let mut opts = comrakOpt();

    // The returned nodes are created in the supplied Arena, and are bound by its lifetime.
    let arena = Arena::new();

    let root = parse_document(&arena, md_content.as_str(), &opts);

    fn iter_nodes<'a, F>(node: &'a AstNode<'a>, f: &F)
    where
        F: Fn(&'a AstNode<'a>),
    {
        f(node);
        for c in node.children() {
            iter_nodes(c, f);
        }
    }

    iter_nodes(root, &|node| {
        match node.data.borrow_mut().value {
            NodeValue::Heading(ref mut head) => {
                println!("Head level:{}", head.level);
            }
            NodeValue::SlideMetaDataBlock(ref mut smd) => {
                let kv_literal = String::from_utf8_lossy(&smd.literal);

                // smd.metadatas = Vec::new();
                let lines = kv_literal.lines();
                for line in lines {
                    // let kv = NodeValue::KV;
                    if let Some((k, v)) = line.split_once(':') {
                        if k.len() <= 0 {
                            println!("break:: {:?}", line);
                            break;
                        }

                        println!("xxxx {:?}:{:?}", k, v);
                        let nodekv = NodeKV {
                            key: k.as_bytes().to_vec(),
                            value: v.as_bytes().to_vec(),
                        };
                        // let kv = NodeValue::KV(nodekv);
                        smd.metadatas.push(nodekv);
                    } else {
                        println!("{:?}", line);
                    }
                }
            }
            NodeValue::CodeBlock(ref mut codeblock) => {
                println!(
                    "{:?} {:?}",
                    String::from_utf8_lossy(&codeblock.info),
                    String::from_utf8_lossy(&codeblock.literal)
                )
            }
            _ => (),
        }
    });

    dump_node(root);

    let mut html = vec![];
    format_html(root, &opts, &mut html).unwrap();

    println!("{}", String::from_utf8(html).unwrap());
}

fn parseSMD1() {
    let filename = "/Users/gangwang/root/code/github/comrak/examples/s1.md";
    let md_content = readfile(filename);

    use comrak::nodes::{AstNode, NodeValue};
    use comrak::{dump_node, format_html, parse_document, Arena, ComrakOptions};

    // The returned nodes are created in the supplied Arena, and are bound by its lifetime.
    let arena = Arena::new();

    let root = parse_document(&arena, md_content.as_str(), &ComrakOptions::default());

    fn iter_nodes<'a, F>(node: &'a AstNode<'a>, f: &F)
    where
        F: Fn(&'a AstNode<'a>),
    {
        f(node);
        for c in node.children() {
            iter_nodes(c, f);
        }
    }

    iter_nodes(root, &|node| {
        match node.data.borrow_mut().value {
            NodeValue::SlideMetaDataBlock(ref mut smd) => {
                let kv_literal = String::from_utf8_lossy(&smd.literal);

                // smd.metadatas = Vec::new();
                let lines = kv_literal.lines();
                for line in lines {
                    // let kv = NodeValue::KV;
                    if let Some((k, v)) = line.split_once(':') {
                        if k.len() <= 0 {
                            println!("break:: {:?}", line);
                            break;
                        }

                        println!("xxxx {:?}:{:?}", k, v);
                        let nodekv = NodeKV {
                            key: k.as_bytes().to_vec(),
                            value: v.as_bytes().to_vec(),
                        };
                        println!("vvvv {:?}", &nodekv);
                        // let kv = NodeValue::KV(nodekv);
                        smd.metadatas.push(nodekv);
                    } else {
                        println!("{:?}", line);
                    }
                }
            }
            NodeValue::CodeBlock(ref mut codeblock) => {
                println!(
                    "{:?} {:?}",
                    String::from_utf8_lossy(&codeblock.info),
                    String::from_utf8_lossy(&codeblock.literal)
                )
            }
            _ => (),
        }
    });

    // dump_node(root);
    println!("--------------------------------1");
    let mut html = vec![];
    let result = format_html(root, &ComrakOptions::default(), &mut html);

    println!("--------------------------------{:?}", result);
    let str = String::from_utf8_lossy(&html);
    println!("{:?}", &str);
}

fn main() {
    parseSMD();
    // parseSMD1();
}
