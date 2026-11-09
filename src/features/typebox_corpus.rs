//! Replays bodies recorded from the TypeScript server (`tools/parity/typebox-corpus.mts`) through
//! every body schema: the 422 `details` must match TypeBox's text, and every accepted body must
//! deserialise into its DTO.

use serde::Deserialize;
use serde_json::Value;

use crate::core::extract::Body;
use crate::features::auth::dto::{LoginBody, LogoutBody, RefreshBody, SwitchGroupBody};
use crate::features::batch::DeleteBatchBody;
use crate::features::guest::dto::{CreateGuestBody, QrBody, UpdateGuestBody};
use crate::features::incident::dto::{CreateIncident, PostReply};
use crate::features::information::dto::{CreateBody, UpdateBody};

#[derive(Debug, Deserialize)]
struct Case {
    dto: String,
    body: Value,
    details: Option<String>,
}

/// Long strings are stored as `"\0<count>*<text>"`.
fn expand(value: Value) -> Value {
    match value {
        Value::String(text) => match text.strip_prefix('\0').and_then(|r| r.split_once('*')) {
            Some((count, unit)) => Value::String(unit.repeat(count.parse().expect("repeat count"))),
            None => Value::String(text),
        },
        Value::Array(items) => Value::Array(items.into_iter().map(expand).collect()),
        Value::Object(map) => Value::Object(map.into_iter().map(|(k, v)| (k, expand(v))).collect()),
        other => other,
    }
}

fn outcome<T: Body>(body: Value) -> Option<String> {
    T::parse(body).err()
}

#[test]
fn schemas_reproduce_the_typescript_details() {
    let cases: Vec<Case> = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/typebox_details.json"
    )))
    .expect("fixture");
    assert!(cases.len() > 1000, "fixture looks truncated");

    let mut failures = Vec::new();
    for case in cases {
        let body = expand(case.body);
        let shown = body.to_string().chars().take(160).collect::<String>();
        let got = match case.dto.as_str() {
            "LoginDto" => outcome::<LoginBody>(body),
            "RefreshTokenDto" => outcome::<RefreshBody>(body),
            "SwitchGroupDto" => outcome::<SwitchGroupBody>(body),
            "LogoutDto" => outcome::<LogoutBody>(body),
            "CreateGuestDto" => outcome::<CreateGuestBody>(body),
            "UpdateGuestDto" => outcome::<UpdateGuestBody>(body),
            "AcknowledgeGuestDto" => outcome::<QrBody>(body),
            "DeleteBatchGuestDto" | "DeleteBatchInformationDto" => outcome::<DeleteBatchBody>(body),
            "CreateIncidentDto" => outcome::<CreateIncident>(body),
            "PostIncidentReplyDto" => outcome::<PostReply>(body),
            "CreateInformationDto" => outcome::<CreateBody>(body),
            "UpdateInformationDto" => outcome::<UpdateBody>(body),
            other => panic!("unmapped DTO {other}"),
        };
        if got != case.details {
            failures.push(format!(
                "{} {shown}\n    want {:?}\n    got  {got:?}",
                case.dto, case.details
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} mismatches, first ones:\n{}",
        failures.len(),
        failures
            .iter()
            .take(15)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}
