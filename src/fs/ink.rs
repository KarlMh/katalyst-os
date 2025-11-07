use crate::terminal::Terminal;
use crate::fs::file::File;

use alloc::vec::Vec;
use alloc::string::{String, ToString};
use alloc::format;
use alloc::vec;

// =========================== AST ===============================

#[derive(Debug)]
pub enum InkNode {
    Title(String, Vec<InkNode>), // now title can have children
    Section(String, Vec<InkNode>),
    List(String, Vec<String>),
    Table(String, Vec<(String, Vec<String>)>),
    Text(String),
}


// =========================== PARSER ============================
//
// This parser handles:
//
// title[name] => {
// section[name] => {
// list[name] => (a,b,c)
// table[name] => { row(x,y,z), ... }
// free text
//
// ===================================================================

pub fn parse_ink(file: &File) -> Vec<InkNode> {
    let text = match core::str::from_utf8(&file.content) {
        Ok(t) => t,
        Err(_) => {
            return vec![InkNode::Text("Invalid UTF-8".to_string())];
        }
    };

    let mut root: Vec<InkNode> = Vec::new();
    let mut stack: Vec<(String, Vec<InkNode>)> = Vec::new();

    for raw in text.lines() {
        let line = raw.trim();
    
        if line.is_empty() {
            continue;
        }
    
        // START BLOCK
        if line.ends_with("=> {") {
            let name = extract_block_name(line);
            let tag = extract_block_type(line);
    
            stack.push((format!("{}:{}", tag, name), Vec::new()));
            continue;
        }
    
        // END BLOCK
        if line == "}" {
            if let Some((tagname, items)) = stack.pop() {
                let (tag, name) = split_tag(&tagname);
    
                let node = match tag {
                    "title"   => InkNode::Title(name.to_string(), items), // include children
                    "section" => InkNode::Section(name.to_string(), items),
                    "list"    => parse_list_block(name, &items),
                    "table"   => parse_table_block(name, &items),
                    _ => InkNode::Text(format!("Unknown tag {}", tag)),
                };
                
    
                if let Some((_pname, parent_items)) = stack.last_mut() {
                    parent_items.push(node);
                } else {
                    root.push(node);
                }
            }
            continue;
        }
    
        // INLINE LIST
        if line.starts_with("list[") && line.contains("] => (") {
            let name = extract_bracket_name(line);
            let items = extract_paren_items(line);
            push_node(&mut stack, InkNode::List(name, items));
            continue;
        }
    
        // INLINE TABLE ROW
        if (line.contains('(') && line.ends_with("),")) ||
           (line.contains('(') && line.ends_with(")") && !line.contains("] => ("))
        {
            push_node(&mut stack, InkNode::Text(line.to_string()));
            continue;
        }
    
        // NORMAL TEXT
        push_node(&mut stack, InkNode::Text(line.to_string()));
    }
    

    root
}

// =========================== Helpers ===========================

fn push_node(stack: &mut Vec<(String, Vec<InkNode>)>, node: InkNode) {
    if let Some((_name, items)) = stack.last_mut() {
        items.push(node);
    }
}

fn extract_block_type(line: &str) -> &str {
    let open = line.find('[').unwrap_or(0);
    &line[..open]
}

fn extract_block_name(line: &str) -> String {
    extract_bracket_name(line)
}

fn extract_bracket_name(line: &str) -> String {
    let start = line.find('[').unwrap() + 1;
    let end   = line.find(']').unwrap();
    line[start..end].trim().to_string()
}

fn extract_paren_items(line: &str) -> Vec<String> {
    let start = line.find('(').unwrap() + 1;
    let end   = line.rfind(')').unwrap();

    line[start..end]
        .split(',')
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

fn split_tag(tag: &str) -> (&str, &str) {
    let mut it = tag.splitn(2, ':');
    (it.next().unwrap(), it.next().unwrap())
}

// ======================= BLOCK LIST PARSE ======================

fn parse_list_block(name: &str, inside: &Vec<InkNode>) -> InkNode {
    let mut items = Vec::new();

    for n in inside {
        if let InkNode::Text(t) = n {
            let trimmed = t.trim().trim_end_matches(',');
            if trimmed.contains('(') {
                items.extend(extract_paren_items(trimmed));
            }
        }
    }

    InkNode::List(name.to_string(), items)
}

// ======================= BLOCK TABLE PARSE ======================

fn parse_table_block(name: &str, inside: &Vec<InkNode>) -> InkNode {
    let mut rows = Vec::new();

    for n in inside {
        if let InkNode::Text(t) = n {
            if let Some(start) = t.find('(') {
                let end = t.rfind(')').unwrap();
                let key = t[..start].trim().trim_end_matches(',').to_string();
                let vals = t[start+1..end]
                    .split(',')
                    .map(|x| x.trim().to_string())
                    .collect();

                rows.push((key, vals));
            }
        }
    }

    InkNode::Table(name.to_string(), rows)
}



pub fn render_ink_vga(term: &mut Terminal, nodes: &[InkNode]) {
    term.clear_screen();

    for node in nodes {
        render_node(term, node, 0);
    }
}

fn render_node(term: &mut Terminal, node: &InkNode, indent: usize) {
    let pad = " ".repeat(indent);

    match node {
        InkNode::Title(name, children) => {
            let up = name.to_uppercase();
            term.write_str(&format!("{}\n", up));
            term.write_str(&format!("{}\n\n", "=".repeat(up.len())));
            for child in children {
                render_node(term, child, indent + 2);
            }
        }
        

        InkNode::Section(name, children) => {
            term.write_str(&format!("{}{}\n", pad, name));
            term.write_str(&format!("{}{}\n\n", pad, "-".repeat(name.len())));
            for child in children {
                render_node(term, child, indent + 2);
            }
        }

        InkNode::List(name, items) => {
            term.write_str(&format!("{}{}:\n", pad, name));
            for it in items {
                term.write_str(&format!("{}- {}\n", pad, it));
            }
            term.write_str("\n");
        }

        InkNode::Table(name, rows) => {
            term.write_str(&format!("{}{}\n", pad, name));
            term.write_str(&format!("{}{}\n", pad, "-".repeat(name.len())));

            // auto-align columns
            let col_count = rows.get(0).map(|(_, v)| v.len()).unwrap_or(0);

            let mut widths = vec![0; col_count + 1];
            for (key, values) in rows {
                widths[0] = widths[0].max(key.len());
                for (i, v) in values.iter().enumerate() {
                    widths[i + 1] = widths[i + 1].max(v.len());
                }
            }

            for (key, values) in rows {
                term.write_str(&format!("{}{: <w$} | ", pad, key, w = widths[0]));
                for (i, v) in values.iter().enumerate() {
                    term.write_str(&format!("{: <w$}", v, w = widths[i + 1]));
                    if i != values.len() - 1 {
                        term.write_str(" | ");
                    }
                }
                term.write_str("\n");
            }
            term.write_str("\n");
        }

        InkNode::Text(txt) => {
            term.write_str(&format!("{}{}\n", pad, txt));
        }
    }
}
