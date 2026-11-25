//! The conversation types as the session log stores them.

use pard_model::types::{Block, Call, Item, ToolResult};
use serde_json::value::RawValue;

#[test]
fn a_logged_conversation_keeps_the_calls_bytes() {
    let input = RawValue::from_string("{\"path\":  \"a.rs\" ,\"offset\":3}".to_owned()).unwrap();
    let items = [
        Item::User("fix it".to_owned()),
        Item::Assistant(vec![
            Block::Thinking {
                text: "look first".to_owned(),
                signature: Some("sig".to_owned()),
            },
            Block::Text("Reading.".to_owned()),
            Block::Call(Call {
                id: "c1".to_owned(),
                tool: "nonexistent".to_owned(),
                input,
            }),
        ]),
        Item::Results(vec![ToolResult {
            id: "c1".to_owned(),
            text: "no such tool".to_owned(),
            failed: true,
        }]),
    ];
    let lines: Vec<String> = items
        .iter()
        .map(|i| serde_json::to_string(i).unwrap())
        .collect();
    assert!(
        lines[1].contains("\"input\":{\"path\":  \"a.rs\" ,\"offset\":3}"),
        "{}",
        lines[1]
    );

    let back: Vec<Item> = lines
        .iter()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let again: Vec<String> = back
        .iter()
        .map(|i| serde_json::to_string(i).unwrap())
        .collect();
    assert_eq!(lines, again);
    let Item::Assistant(blocks) = &back[1] else {
        panic!("{:?}", back[1])
    };
    let Block::Call(call) = &blocks[2] else {
        panic!("{:?}", blocks[2])
    };
    assert_eq!(call.input.get(), "{\"path\":  \"a.rs\" ,\"offset\":3}");
}

#[test]
fn thinking_without_a_signature_omits_it() {
    let item = Item::Assistant(vec![Block::Thinking {
        text: "t".to_owned(),
        signature: None,
    }]);
    assert_eq!(
        serde_json::to_string(&item).unwrap(),
        "{\"assistant\":[{\"thinking\":{\"text\":\"t\"}}]}"
    );
}
