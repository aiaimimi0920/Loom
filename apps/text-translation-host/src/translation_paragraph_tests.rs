use crate::translation::translate_with;
use crate::translation_input::{TextBlock, TranslationInput, MAX_TEXT_CHARS};
use crate::translation_paragraphs::group;
use serde_json::{json, Value};

fn block(text: &str, left: f64, top: f64, width: f64, height: f64) -> TextBlock {
    TextBlock {
        text: text.to_owned(),
        left,
        top,
        width,
        height,
        text_color: "#f8fafc".to_owned(),
        background_color: "#140b42".to_owned(),
    }
}

fn news_blocks() -> Vec<TextBlock> {
    let mut blocks = vec![
        block("Trump warns", 19.0, 37.0, 278.0, 53.0),
        block("Tn UN", 309.0, 39.0, 115.0, 40.0),
        block("speech he could", 18.0, 98.0, 340.0, 51.0),
        block("annihilate' lran", 31.0, 156.0, 325.0, 47.0),
        block("without", 19.0, 217.0, 170.0, 45.0),
        block("peace deal", 191.0, 219.0, 230.0, 46.0),
        block(
            " US President Donald Trump used his UN General",
            19.0,
            284.0,
            435.0,
            26.0,
        ),
        block(
            "Assembly speech on Tuesday to make the case for",
            22.0,
            315.0,
            437.0,
            23.0,
        ),
        block(
            "his war on lran, arguing he has prevented Tehran",
            18.0,
            342.0,
            434.0,
            30.0,
        ),
        block(
            "from obtaining a nuclear weapon and warning that",
            19.0,
            373.0,
            453.0,
            28.0,
        ),
        block(
            "he could \"annihilate the Islamic Republic\" if no",
            19.0,
            404.0,
            416.0,
            25.0,
        ),
        block(
            "deal is reached to end the conflict.",
            21.0,
            435.0,
            302.0,
            22.0,
        ),
        block("52 mins ago", 19.0, 465.0, 100.0, 24.0),
    ];
    blocks[1].text_color = "#fafbfa".to_owned();
    blocks[12].text_color = "#969398".to_owned();
    blocks
}

#[test]
fn wrapped_news_is_translated_as_complete_headline_body_and_timestamp() {
    let blocks = news_blocks();
    let original = blocks
        .iter()
        .map(|block| block.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let input: TranslationInput = serde_json::from_value(json!({
        "text": original, "targetLanguage": "zh-CN", "sourceWidth": 476, "sourceHeight": 569,
        "sourceRevision": 7,
        "sourceAttachment": { "attachmentId": "neuro.official/ocr.result", "revision": 37 },
        "textBlocks": blocks,
    }))
    .unwrap();
    let mut requests = Vec::new();
    let output = translate_with(input, |_, request, _, _| {
        let request: Value = serde_json::from_str(request)?;
        let id = request["texts"][0][0].as_u64().unwrap();
        requests.push((id, request["texts"][0][1].as_str().unwrap().to_owned()));
        let translated = match id {
            0 => "特朗普在联合国演讲中警告说，没有和平协议他可以消灭伊朗。",
            6 => "美国总统在联合国大会演讲中警告，如果无法达成结束冲突的协议，他可以消灭伊斯兰共和国。",
            12 => "52分钟前",
            _ => panic!("sentence context was split at source block {id}"),
        };
        Ok(json!({ "translations": [[id, translated]] }).to_string())
    }).unwrap();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests[0].1,
        "Trump warns Tn UN speech he could annihilate' lran without peace deal"
    );
    assert!(requests[1].1.contains("UN General Assembly"));
    assert!(requests[1]
        .1
        .ends_with("if no deal is reached to end the conflict."));
    assert_eq!(output.original_text, original);
    assert_eq!(output.source_attachment.as_ref().unwrap().revision, 37);
    assert_eq!(
        output
            .text_blocks
            .iter()
            .map(|block| block.source_block_indices.clone())
            .collect::<Vec<_>>(),
        vec![vec![0, 1, 2, 3, 4, 5], vec![6, 7, 8, 9, 10, 11], vec![12]]
    );
    let headline = &output.text_blocks[0];
    assert_eq!(
        (
            headline.source.left,
            headline.source.top,
            headline.source.width,
            headline.source.height
        ),
        (18.0, 37.0, 406.0, 228.0)
    );
    let body = &output.text_blocks[1];
    assert_eq!(
        (
            body.source.left,
            body.source.top,
            body.source.width,
            body.source.height
        ),
        (18.0, 284.0, 454.0, 173.0)
    );
    assert!(headline.source_font_size > body.source_font_size * 1.5);
    assert!(headline.source_font_size < 53.0);
    let encoded = serde_json::to_value(&output).unwrap();
    assert_eq!(
        encoded["textBlocks"][1]["sourceBlockIndices"],
        json!([6, 7, 8, 9, 10, 11])
    );
    assert!(encoded["textBlocks"][0].get("sourceFontSize").is_none());
    let children = output.surface_scene["children"].as_array().unwrap();
    assert_eq!(children.len(), 3);
    for (child, block) in children.iter().zip(&output.text_blocks) {
        assert_eq!(child["props"]["text"], block.translated_text);
        assert_eq!(child["style"]["lineHeight"], "1.2em");
        assert!(child["style"]["fontSize"]
            .as_str()
            .unwrap()
            .starts_with("min("));
        assert_eq!(child["layout"]["overflowY"], "hidden");
    }
}

#[test]
fn interleaved_columns_never_share_a_paragraph_or_lose_source_indices() {
    let blocks = vec![
        block("The left column begins here", 10.0, 10.0, 190.0, 20.0),
        block("The right column starts here", 240.0, 10.0, 190.0, 20.0),
        block("and continues on this line", 10.0, 34.0, 190.0, 20.0),
        block("and continues on this line", 240.0, 34.0, 190.0, 20.0),
    ];
    let groups = group(&blocks);
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0].source_block_indices, vec![0, 2]);
    assert_eq!(groups[1].source_block_indices, vec![1, 3]);
    assert_eq!(groups[0].source.width, 190.0);
    assert_eq!(groups[1].source.width, 190.0);
}

#[test]
fn vertically_jittered_fragments_are_joined_in_reading_order() {
    let blocks = vec![
        block("We can", 0.0, 10.0, 60.0, 20.0),
        block("today", 130.0, 10.0, 50.0, 20.0),
        block("continue", 65.0, 11.0, 60.0, 20.0),
    ];
    let groups = group(&blocks);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].source.text, "We can continue today");
    assert_eq!(groups[0].source_block_indices, vec![0, 2, 1]);
}

#[test]
fn gaps_lists_and_stacked_controls_stay_separate() {
    for blocks in [
        vec![
            block("A complete paragraph ends here.", 0.0, 0.0, 240.0, 20.0),
            block("A separate paragraph starts here.", 0.0, 45.0, 240.0, 20.0),
        ],
        vec![
            block("1. First item in this list", 0.0, 0.0, 220.0, 20.0),
            block("2. Next item in this list", 0.0, 24.0, 220.0, 20.0),
        ],
        vec![
            block("Save", 0.0, 0.0, 60.0, 20.0),
            block("Cancel", 0.0, 24.0, 65.0, 20.0),
        ],
    ] {
        assert_eq!(group(&blocks).len(), 2, "{blocks:?}");
    }
}

#[test]
fn paragraph_rectangle_does_not_cover_an_unrelated_side_note() {
    let mut blocks = vec![
        block("The paragraph starts here", 0.0, 0.0, 200.0, 20.0),
        block("finishes here.", 0.0, 24.0, 100.0, 20.0),
        block("side note", 110.0, 24.0, 90.0, 20.0),
    ];
    blocks[2].background_color = "#ffffff".to_owned();
    assert_eq!(group(&blocks).len(), 3);
}

#[test]
fn source_budget_is_enforced_before_grouping_or_calling_a_model() {
    let blocks = vec![
        block(&"a".repeat(MAX_TEXT_CHARS / 2), 0.0, 0.0, 10_000.0, 20.0),
        block(&"b".repeat(MAX_TEXT_CHARS / 2), 0.0, 24.0, 10_000.0, 20.0),
    ];
    assert_eq!(group(&blocks).len(), 2);
    let input = serde_json::from_value(json!({
        "text": "source", "targetLanguage": "zh-CN", "sourceWidth": 400, "sourceHeight": 100,
        "textBlocks": [block(&"a".repeat(MAX_TEXT_CHARS + 1), 0.0, 0.0, 100.0, 20.0)],
    }))
    .unwrap();
    assert!(translate_with(input, |_, _, _, _| panic!("oversized source reached model")).is_err());
}
