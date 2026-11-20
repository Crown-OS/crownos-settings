use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

/// A Markdown block, its inline formatting flattened to plain text.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading {
        level: u8,
        text: String,
    },
    Paragraph(String),
    Quote(String),
    Item {
        depth: usize,
        marker: String,
        text: String,
    },
    Code(String),
    Rule,
}

const CELL_GAP: &str = "    ";

pub fn blocks(source: &str) -> Vec<Block> {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut builder = Builder::default();
    Parser::new_ext(source, options).for_each(|event| builder.feed(event));
    builder.blocks
}

#[derive(Default)]
struct Builder {
    blocks: Vec<Block>,
    text: String,
    markers: Vec<String>,
    numbering: Vec<Option<u64>>,
    quote_depth: usize,
}

impl Builder {
    fn feed(&mut self, event: Event<'_>) {
        match event {
            Event::Text(text) | Event::Code(text) => self.text.push_str(&text),
            Event::SoftBreak => self.text.push(' '),
            Event::HardBreak => self.text.push('\n'),
            Event::Rule => self.blocks.push(Block::Rule),
            Event::TaskListMarker(done) => self.text.push_str(if done { "☑ " } else { "☐ " }),
            Event::Start(Tag::List(first)) => {
                self.flush_item();
                self.numbering.push(first);
            }
            Event::End(TagEnd::List(_)) => {
                self.numbering.pop();
            }
            Event::Start(Tag::Item) => {
                let marker = match self.numbering.last_mut() {
                    Some(Some(number)) => {
                        *number += 1;
                        format!("{}.", *number - 1)
                    }
                    _ => "•".to_owned(),
                };
                self.markers.push(marker);
            }
            Event::End(TagEnd::Item) => {
                self.flush_item();
                self.markers.pop();
            }
            Event::Start(Tag::BlockQuote(_)) => self.quote_depth += 1,
            Event::End(TagEnd::BlockQuote(_)) => {
                self.quote_depth = self.quote_depth.saturating_sub(1)
            }
            Event::End(TagEnd::Heading(level)) => {
                let text = self.take();
                self.blocks.push(Block::Heading {
                    level: heading_level(level),
                    text,
                });
            }
            Event::End(TagEnd::CodeBlock) => {
                let code = self.take().trim_end().to_owned();
                self.blocks.push(Block::Code(code));
            }
            Event::End(TagEnd::Paragraph) => self.flush_paragraph(),
            Event::End(TagEnd::TableCell) => self.text.push_str(CELL_GAP),
            Event::End(TagEnd::TableHead | TagEnd::TableRow) => {
                let row = self.take().trim_end().to_owned();
                self.blocks.push(Block::Paragraph(row));
            }
            _ => {}
        }
    }

    fn take(&mut self) -> String {
        std::mem::take(&mut self.text)
    }

    fn flush_paragraph(&mut self) {
        if !self.markers.is_empty() {
            return self.flush_item();
        }
        let text = self.take();
        let block = if self.quote_depth > 0 {
            Block::Quote(text)
        } else {
            Block::Paragraph(text)
        };
        self.blocks.push(block);
    }

    fn flush_item(&mut self) {
        if self.text.trim().is_empty() {
            return;
        }
        let Some(marker) = self.markers.last().cloned() else {
            return;
        };
        let text = self.take();
        self.blocks.push(Block::Item {
            depth: self.markers.len() - 1,
            marker,
            text,
        });
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_blocks_are_flattened_in_order() {
        let source = "# Title\n\nSome *bold* text.\n\n- one\n- two\n  1. nested\n\n> quoted\n\n```\ncode\n```\n\n---\n";
        let item = |depth, marker: &str, text: &str| Block::Item {
            depth,
            marker: marker.into(),
            text: text.into(),
        };
        assert_eq!(
            blocks(source),
            vec![
                Block::Heading {
                    level: 1,
                    text: "Title".into()
                },
                Block::Paragraph("Some bold text.".into()),
                item(0, "•", "one"),
                item(0, "•", "two"),
                item(1, "1.", "nested"),
                Block::Quote("quoted".into()),
                Block::Code("code".into()),
                Block::Rule,
            ]
        );
    }
}
