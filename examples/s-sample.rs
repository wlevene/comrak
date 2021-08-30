use std::{fmt::Debug, fs::File, io::Read};

use comrak::nodes::NodeKV;

// Samples used in the README.  Wanna make sure they work as advertised.

extern crate comrak;

fn readfile(filename: &str) -> String {
    let mut f = File::open(filename).expect("file not found");
    let mut contents = String::new();
    f.read_to_string(&mut contents)
        .expect("something went wrong reading the file");
    contents
}

fn parse() {
    let filename = "/Users/gangwang/root/code/github/comrak/examples/s.md";
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
        if let NodeValue::SlideMetaDataBlock(ref mut smd) = node.data.borrow_mut().value {
            let kv_literal = String::from_utf8_lossy(&smd.literal);

            // smd.metadatas = Vec::new();
            let lines = kv_literal.lines();
            for line in lines {
                // let kv = NodeValue::KV;
                if let Some((k, v)) = line.split_once(':') {
                    if k.len() <= 0 {
                        break;
                    }

                    println!("{:?}:{:?}", k, v);
                    let nodekv = NodeKV {
                        key: k.as_bytes().to_vec(),
                        value: v.as_bytes().to_vec(),
                    };
                    // println!("{:?}", &nodekv);
                    // let kv = NodeValue::KV(nodekv);
                    smd.metadatas.push(nodekv);
                }
            }
        }
    });

    dump_node(root);
}

fn main() {
    parse();
}
