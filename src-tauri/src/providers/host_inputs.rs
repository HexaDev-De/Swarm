//! Bounded, allowlisted structured input crossing the provider/UI boundary.
use serde_json::{json, Map, Value};
use std::{collections::HashSet, io};

const PAYLOAD_LIMIT: usize = 32 * 1024;
const TEXT_LIMIT: usize = 4096;
const LABEL_LIMIT: usize = 256;
const FIELD_LIMIT: usize = 20;

// Count serialized bytes without allocating a second copy of untrusted input.
fn budget(value: &Value) -> Result<(), String> {
    struct Counter(usize);
    impl io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > PAYLOAD_LIMIT.saturating_sub(self.0) {
                return Err(io::Error::other("Structured input exceeds the size limit"));
            }
            self.0 += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Counter(0), value)
        .map_err(|_| "Structured input exceeds the size limit".into())
}

fn text(value: &Value, limit: usize, nonempty: bool) -> Result<&str, String> {
    value
        .as_str()
        .filter(|text| text.len() <= limit && (!nonempty || !text.is_empty()))
        .ok_or_else(|| "Structured input contains an invalid or oversized text field".into())
}

fn key(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 128
        || value.chars().any(char::is_control)
        || ["__proto__", "prototype", "constructor"].contains(&value)
    {
        return Err("Structured input contains an unsupported field ID".into());
    }
    Ok(())
}

fn flag(value: Option<&Value>) -> Result<bool, String> {
    match value {
        None => Ok(false),
        Some(value) => value
            .as_bool()
            .ok_or_else(|| "Structured input contains an invalid boolean flag".into()),
    }
}

fn allowed(object: &Map<String, Value>, keys: &[&str]) -> Result<(), String> {
    if object.keys().any(|key| !keys.contains(&key.as_str())) {
        return Err("Structured input uses an unsupported field or schema constraint".into());
    }
    Ok(())
}

fn object(value: &Value) -> Result<&Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| "Structured input must be an object".into())
}

fn questions(params: &Value) -> Result<Value, String> {
    let questions = params["questions"]
        .as_array()
        .filter(|questions| !questions.is_empty() && questions.len() <= 3)
        .ok_or("Structured input supports one to three questions")?;
    let mut ids = HashSet::new();
    let mut normalized = Vec::with_capacity(questions.len());
    for question in questions {
        let id = text(&question["id"], 128, true)?;
        key(id)?;
        if !ids.insert(id) {
            return Err("Structured input contains duplicate question IDs".into());
        }
        let options = match question.get("options") {
            None | Some(Value::Null) => Value::Null,
            Some(Value::Array(options)) if options.len() <= FIELD_LIMIT => {
                let mut labels = HashSet::new();
                let mut output = Vec::with_capacity(options.len());
                for option in options {
                    let label = text(&option["label"], LABEL_LIMIT, true)?;
                    if !labels.insert(label) {
                        return Err("Structured input contains duplicate option labels".into());
                    }
                    output.push(json!({"label":label,"description":text(&option["description"], 1024, false)?}));
                }
                Value::Array(output)
            }
            _ => return Err("Structured input has too many or invalid options".into()),
        };
        normalized.push(json!({
            "id":id,
            "header":text(&question["header"],128,false)?,
            "question":text(&question["question"],TEXT_LIMIT,true)?,
            "isOther":flag(question.get("isOther"))?,
            "isSecret":flag(question.get("isSecret"))?,
            "options":options
        }));
    }
    Ok(json!({"kind":"questions","questions":normalized}))
}

fn copy_description(
    source: &Map<String, Value>,
    target: &mut Map<String, Value>,
) -> Result<(), String> {
    for (name, limit) in [("title", LABEL_LIMIT), ("description", 2048)] {
        if let Some(value) = source.get(name) {
            target.insert(name.into(), json!(text(value, limit, false)?));
        }
    }
    Ok(())
}

fn choices(value: &Value) -> Result<Vec<Value>, String> {
    let values = value
        .as_array()
        .filter(|values| !values.is_empty() && values.len() <= FIELD_LIMIT)
        .ok_or("Form enums must contain one to twenty options")?;
    let mut seen = HashSet::new();
    let mut output = Vec::with_capacity(values.len());
    for value in values {
        let choice = text(value, LABEL_LIMIT, false)?;
        if !seen.insert(choice) {
            return Err("Form enum contains duplicate options".into());
        }
        output.push(json!(choice));
    }
    Ok(output)
}

fn finite_number(value: &Value) -> Result<f64, String> {
    value
        .as_f64()
        .filter(|number| number.is_finite() && number.abs() <= 9_007_199_254_740_991.0)
        .ok_or_else(|| "Form number is outside the supported range".into())
}

fn property(value: &Value) -> Result<Value, String> {
    let source = object(value)?;
    let kind = text(&value["type"], 16, true)?;
    let mut output = Map::new();
    output.insert("type".into(), json!(kind));
    copy_description(source, &mut output)?;
    match kind {
        "string" => {
            allowed(
                source,
                &[
                    "type",
                    "title",
                    "description",
                    "default",
                    "minLength",
                    "maxLength",
                    "enum",
                    "enumNames",
                    "oneOf",
                ],
            )?;
            for name in ["minLength", "maxLength"] {
                if let Some(value) = source.get(name) {
                    let length = value
                        .as_u64()
                        .filter(|length| *length <= TEXT_LIMIT as u64)
                        .ok_or("Form string length exceeds the supported limit")?;
                    output.insert(name.into(), json!(length));
                }
            }
            if output.get("minLength").and_then(Value::as_u64).unwrap_or(0)
                > output
                    .get("maxLength")
                    .and_then(Value::as_u64)
                    .unwrap_or(TEXT_LIMIT as u64)
            {
                return Err("Form string length constraints conflict".into());
            }
            if let Some(enumeration) = source.get("enum") {
                output.insert("enum".into(), Value::Array(choices(enumeration)?));
                if let Some(names) = source.get("enumNames") {
                    let names = names
                        .as_array()
                        .filter(|names| names.len() == output["enum"].as_array().unwrap().len())
                        .ok_or("Form enum labels do not match its options")?;
                    let names = names
                        .iter()
                        .map(|name| text(name, LABEL_LIMIT, false).map(|name| json!(name)))
                        .collect::<Result<Vec<_>, _>>()?;
                    output.insert("enumNames".into(), Value::Array(names));
                }
            } else if source.contains_key("enumNames") {
                return Err("Form enum labels require options".into());
            }
            if let Some(one_of) = source.get("oneOf") {
                if source.contains_key("enum") {
                    return Err("Form enum constraints conflict".into());
                }
                let options = one_of
                    .as_array()
                    .filter(|items| !items.is_empty() && items.len() <= FIELD_LIMIT)
                    .ok_or("Form enum has too many or invalid options")?;
                let mut values = Vec::with_capacity(options.len());
                let mut names = Vec::with_capacity(options.len());
                for option in options {
                    allowed(object(option)?, &["const", "title"])?;
                    values.push(json!(text(&option["const"], LABEL_LIMIT, false)?));
                    names.push(json!(text(
                        option.get("title").unwrap_or(&option["const"]),
                        LABEL_LIMIT,
                        false
                    )?));
                }
                output.insert("enum".into(), Value::Array(choices(&Value::Array(values))?));
                output.insert("enumNames".into(), Value::Array(names));
            }
        }
        "number" | "integer" => {
            allowed(
                source,
                &[
                    "type",
                    "title",
                    "description",
                    "default",
                    "minimum",
                    "maximum",
                ],
            )?;
            for name in ["minimum", "maximum"] {
                if let Some(value) = source.get(name) {
                    finite_number(value)?;
                    output.insert(name.into(), value.clone());
                }
            }
            if let (Some(min), Some(max)) = (output.get("minimum"), output.get("maximum")) {
                if finite_number(min)? > finite_number(max)? {
                    return Err("Form numeric constraints conflict".into());
                }
            }
        }
        "boolean" => allowed(source, &["type", "title", "description", "default"])?,
        _ => return Err("This form field type is not supported".into()),
    }
    let mut output = Value::Object(output);
    if let Some(default) = source.get("default") {
        validate_scalar(&output, default)?;
        output["default"] = default.clone();
    }
    Ok(output)
}

fn schema(value: &Value) -> Result<Value, String> {
    let source = object(value)?;
    allowed(
        source,
        &[
            "$schema",
            "type",
            "title",
            "description",
            "properties",
            "required",
            "additionalProperties",
        ],
    )?;
    if value["type"] != "object" {
        return Err("Only simple object forms are supported".into());
    }
    if let Some(dialect) = source.get("$schema") {
        text(dialect, 512, true)?;
    }
    let properties = object(&value["properties"])?;
    if properties.len() > FIELD_LIMIT {
        return Err("Forms support at most twenty fields".into());
    }
    let mut fields = Map::new();
    for (name, value) in properties {
        key(name)?;
        fields.insert(name.clone(), property(value)?);
    }
    let mut result = json!({"type":"object","properties":fields});
    copy_description(source, result.as_object_mut().unwrap())?;
    if let Some(required) = source.get("required") {
        let required = required
            .as_array()
            .filter(|required| required.len() <= FIELD_LIMIT)
            .ok_or("Form required fields are invalid")?;
        let mut seen = HashSet::new();
        let mut names = Vec::with_capacity(required.len());
        for required in required {
            let name = text(required, 128, true)?;
            if !properties.contains_key(name) || !seen.insert(name) {
                return Err("Form required fields are unknown or duplicated".into());
            }
            names.push(json!(name));
        }
        result["required"] = Value::Array(names);
    }
    if let Some(additional) = source.get("additionalProperties") {
        if additional != false {
            return Err("Forms with arbitrary additional fields are not supported".into());
        }
        result["additionalProperties"] = json!(false);
    }
    Ok(result)
}

pub(super) fn normalize(request: &Value) -> Result<Value, String> {
    let params = &request["params"];
    let data = match request["method"].as_str() {
        Some("item/tool/requestUserInput") => questions(params)?,
        Some("mcpServer/elicitation/request") => {
            let message = text(&params["message"], TEXT_LIMIT, false)?;
            match params["mode"].as_str() {
                Some("form" | "openai/form" | "openaiForm") => {
                    json!({"kind":"form","message":message,"schema":schema(&params["requestedSchema"])?})
                }
                Some("url") => {
                    let url = text(&params["url"], TEXT_LIMIT, true)?;
                    let parsed =
                        tauri::Url::parse(url).map_err(|_| "The confirmation URL is invalid")?;
                    if !matches!(parsed.scheme(), "http" | "https")
                        || !parsed.has_host()
                        || !parsed.username().is_empty()
                        || parsed.password().is_some()
                        || url
                            .chars()
                            .any(|character| character.is_control() || character.is_whitespace())
                        || url.contains('\\')
                    {
                        return Err("Confirmation URLs must use HTTP(S) without credentials".into());
                    }
                    json!({"kind":"url","message":message,"url":url})
                }
                _ => return Err("This elicitation mode is not supported".into()),
            }
        }
        _ => return Err("This structured input request is not supported".into()),
    };
    budget(&data)?;
    Ok(data)
}

fn validate_scalar(schema: &Value, value: &Value) -> Result<(), String> {
    match schema["type"].as_str() {
        Some("string") => {
            let answer = text(value, TEXT_LIMIT, false)?;
            let length = answer.chars().count() as u64;
            if schema["minLength"]
                .as_u64()
                .is_some_and(|minimum| length < minimum)
                || schema["maxLength"]
                    .as_u64()
                    .is_some_and(|maximum| length > maximum)
                || schema["enum"]
                    .as_array()
                    .is_some_and(|options| !options.iter().any(|option| option == value))
            {
                return Err("A form answer does not satisfy its field constraints".into());
            }
        }
        Some("number" | "integer") => {
            let number = finite_number(value)?;
            if (schema["type"] == "integer" && number.fract() != 0.0)
                || schema["minimum"]
                    .as_f64()
                    .is_some_and(|minimum| number < minimum)
                || schema["maximum"]
                    .as_f64()
                    .is_some_and(|maximum| number > maximum)
            {
                return Err("A form number does not satisfy its field constraints".into());
            }
        }
        Some("boolean") if value.is_boolean() => {}
        _ => return Err("A form answer has the wrong type".into()),
    }
    Ok(())
}

pub(super) fn answer(request: &Value, response: Value) -> Result<Value, String> {
    budget(&response)?;
    let data = normalize(request)?;
    let response_object = object(&response)?;
    let result = if data["kind"] == "questions" {
        allowed(response_object, &["answers"])?;
        let answers = object(&response["answers"])?;
        let questions = data["questions"].as_array().unwrap();
        if !answers.is_empty() && answers.len() != questions.len() {
            return Err("Answer every question or cancel the request".into());
        }
        let mut output = Map::new();
        for (id, value) in answers {
            let question = questions
                .iter()
                .find(|question| question["id"] == id.as_str())
                .ok_or("Response contains an unknown question ID")?;
            let answers = value
                .as_array()
                .filter(|answers| answers.len() == 1)
                .ok_or("Each question requires exactly one answer")?;
            let answer = text(&answers[0], TEXT_LIMIT, true)?;
            let options = question["options"].as_array();
            if question["isOther"] != true
                && options.is_some_and(|options| {
                    !options.is_empty() && !options.iter().any(|option| option["label"] == answer)
                })
            {
                return Err("Response contains an option that was not offered".into());
            }
            output.insert(id.clone(), json!({"answers":[answer]}));
        }
        json!({"answers":output})
    } else {
        allowed(response_object, &["action", "content"])?;
        let action = response["action"]
            .as_str()
            .filter(|action| ["accept", "decline", "cancel"].contains(action))
            .ok_or("Choose accept, decline, or cancel")?;
        let content = if action == "accept" && data["kind"] == "form" {
            let content = object(&response["content"])?;
            let schema = &data["schema"];
            let properties = schema["properties"].as_object().unwrap();
            if let Some(required) = schema["required"].as_array() {
                for name in required {
                    if !content.contains_key(name.as_str().unwrap()) {
                        return Err("A required form field is missing".into());
                    }
                }
            }
            for (name, value) in content {
                let field = properties
                    .get(name)
                    .ok_or("Response contains an unknown form field")?;
                validate_scalar(field, value)?;
            }
            Value::Object(content.clone())
        } else {
            Value::Null
        };
        json!({"action":action,"content":content,"_meta":null})
    };
    budget(&result)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn questions() -> Value {
        json!({"method":"item/tool/requestUserInput","params":{"questions":[{"id":"choice","header":"Approach","question":"Choose an approach","isOther":false,"isSecret":false,"options":[{"label":"Small change","description":"Keep the change focused"},{"label":"Full revision","description":"Update the whole module"}]}]}})
    }

    fn form() -> Value {
        json!({"method":"mcpServer/elicitation/request","params":{"mode":"form","message":"Configure the task","requestedSchema":{"type":"object","properties":{"name":{"type":"string","minLength":2,"maxLength":20},"count":{"type":"integer","minimum":1,"maximum":5},"enabled":{"type":"boolean"},"region":{"type":"string","enum":["east","west"]}},"required":["name","count","region"]}}})
    }

    fn url(value: &str) -> Value {
        json!({"method":"mcpServer/elicitation/request","params":{"mode":"url","message":"Finish connection","url":value,"elicitationId":"connection"}})
    }

    #[test]
    fn question_labels_and_ids_are_preserved_and_wrapped_for_the_protocol() {
        let request = questions();
        let data = normalize(&request).unwrap();
        assert_eq!(data["kind"], "questions");
        assert_eq!(data["questions"], request["params"]["questions"]);
        assert_eq!(
            answer(&request, json!({"answers":{"choice":["Full revision"]}})).unwrap(),
            json!({"answers":{"choice":{"answers":["Full revision"]}}})
        );
        assert_eq!(
            answer(&request, json!({"answers":{}})).unwrap(),
            json!({"answers":{}})
        );
    }

    #[test]
    fn answers_reject_unknown_ids_choices_and_oversize_values() {
        let request = questions();
        for response in [
            json!({"answers":{"unknown":["Small change"]}}),
            json!({"answers":{"choice":["Unlisted"]}}),
            json!({"answers":{"choice":[]}}),
            json!({"answers":{"choice":["Small change","Full revision"]}}),
            json!({"answers":{"choice":["x".repeat(4097)]}}),
        ] {
            assert!(answer(&request, response).is_err());
        }
    }

    #[test]
    fn secret_freeform_and_other_answers_preserve_exact_user_text() {
        let mut request = questions();
        request["params"]["questions"][0]["isOther"] = json!(true);
        assert_eq!(
            answer(&request, json!({"answers":{"choice":["My approach"]}})).unwrap()["answers"]
                ["choice"]["answers"][0],
            "My approach"
        );
        request["params"]["questions"][0]["isSecret"] = json!(true);
        request["params"]["questions"][0]["isOther"] = json!(false);
        request["params"]["questions"][0]["options"] = Value::Null;
        assert_eq!(
            normalize(&request).unwrap()["questions"][0]["isSecret"],
            true
        );
        assert_eq!(
            answer(&request, json!({"answers":{"choice":[" secret value "]}})).unwrap()["answers"]
                ["choice"]["answers"][0],
            " secret value "
        );
    }

    #[test]
    fn question_requests_reject_duplicate_and_oversize_correlators_or_options() {
        for field in ["id", "header", "question"] {
            let mut request = questions();
            request["params"]["questions"][0][field] = json!("x".repeat(5000));
            assert!(normalize(&request).is_err());
        }
        let mut request = questions();
        request["params"]["questions"][0]["options"][0]["label"] = json!("x".repeat(5000));
        assert!(normalize(&request).is_err());
        let mut request = questions();
        let question = request["params"]["questions"][0].clone();
        request["params"]["questions"] = json!([question.clone(), question]);
        assert!(normalize(&request).is_err());
    }

    #[test]
    fn forms_validate_required_types_ranges_and_enum_values() {
        let request = form();
        let data = normalize(&request).unwrap();
        assert_eq!(data["kind"], "form");
        assert_eq!(data["schema"], request["params"]["requestedSchema"]);
        let content = json!({"name":"Task","count":2,"region":"west","enabled":true});
        assert_eq!(
            answer(&request, json!({"action":"accept","content":content})).unwrap(),
            json!({"action":"accept","content":content,"_meta":null})
        );
        for content in [
            json!({}),
            json!({"name":"X","count":2,"region":"west"}),
            json!({"name":"Task","count":2.5,"region":"west"}),
            json!({"name":"Task","count":6,"region":"west"}),
            json!({"name":"Task","count":2,"region":"north"}),
            json!({"name":"Task","count":2,"region":"west","extra":true}),
        ] {
            assert!(answer(&request, json!({"action":"accept","content":content})).is_err());
        }
    }

    #[test]
    fn forms_reject_unsupported_nested_schemas_and_validate_cancel_action() {
        for schema in [
            json!({"type":"object","properties":{"nested":{"type":"object"}}}),
            json!({"type":"object","properties":{"nested":{"$ref":"https://example.com/schema"}}}),
            json!({"type":"object","properties":{"text":{"type":"string","pattern":".*"}}}),
        ] {
            let mut request = form();
            request["params"]["requestedSchema"] = schema;
            assert!(normalize(&request).is_err());
        }
        assert_eq!(
            answer(&form(), json!({"action":"cancel"})).unwrap(),
            json!({"action":"cancel","content":null,"_meta":null})
        );
        assert!(answer(&form(), json!({"action":"yes"})).is_err());
    }

    #[test]
    fn url_confirmation_accepts_only_http_urls_without_credentials() {
        let request = url("https://example.com/connect?state=opaque");
        assert_eq!(
            normalize(&request).unwrap()["url"],
            request["params"]["url"]
        );
        assert_eq!(
            answer(&request, json!({"action":"accept"})).unwrap(),
            json!({"action":"accept","content":null,"_meta":null})
        );
        for invalid in [
            "javascript:alert(1)",
            "file:///tmp/a",
            "https://user:pass@example.com/",
            "https://user@example.com/",
            "not a url",
        ] {
            assert!(normalize(&url(invalid)).is_err());
        }
    }

    #[test]
    fn count_limits_and_aggregate_payload_budget_are_enforced() {
        let question = questions()["params"]["questions"][0].clone();
        let mut request = questions();
        request["params"]["questions"] = Value::Array(
            (0..4)
                .map(|id| {
                    let mut question = question.clone();
                    question["id"] = json!(format!("question-{id}"));
                    question
                })
                .collect(),
        );
        assert!(normalize(&request).is_err());
        let options = |count: usize| {
            Value::Array(
                (0..count)
                    .map(
                        |id| json!({"label":format!("option-{id}"),"description":"x".repeat(1024)}),
                    )
                    .collect(),
            )
        };
        request = questions();
        request["params"]["questions"][0]["options"] = options(21);
        assert!(normalize(&request).is_err());
        request["params"]["questions"][0]["options"] = options(20);
        assert!(normalize(&request).is_ok());
        let mut second = request["params"]["questions"][0].clone();
        second["id"] = json!("second");
        request["params"]["questions"]
            .as_array_mut()
            .unwrap()
            .push(second);
        assert!(
            normalize(&request).is_err(),
            "individually valid fields must still fit the IPC budget"
        );
        assert!(answer(
            &form(),
            json!({"action":"cancel","content":"x".repeat(PAYLOAD_LIMIT)})
        )
        .is_err());
        let mut request = form();
        request["params"]["requestedSchema"]["properties"] = Value::Object(
            (0..21)
                .map(|id| (format!("field-{id}"), json!({"type":"boolean"})))
                .collect(),
        );
        request["params"]["requestedSchema"]["required"] = json!([]);
        assert!(normalize(&request).is_err());
    }

    #[test]
    fn titled_enum_options_keep_exact_values_and_validate_defaults() {
        let mut request = form();
        request["params"]["requestedSchema"]["properties"]["region"] = json!({"type":"string","oneOf":[{"const":"east","title":"East Coast"},{"const":"west","title":"West Coast"}],"default":"west"});
        let normalized = normalize(&request).unwrap();
        assert_eq!(
            normalized["schema"]["properties"]["region"]["enum"],
            json!(["east", "west"])
        );
        assert_eq!(
            normalized["schema"]["properties"]["region"]["enumNames"],
            json!(["East Coast", "West Coast"])
        );
        assert!(answer(
            &request,
            json!({"action":"accept","content":{"name":"Task","count":2,"region":"West Coast"}})
        )
        .is_err());
        request["params"]["requestedSchema"]["properties"]["region"]["default"] = json!("north");
        assert!(normalize(&request).is_err());
    }

    #[test]
    fn partial_question_answers_and_unsafe_object_keys_are_rejected() {
        let mut request = questions();
        let mut second = request["params"]["questions"][0].clone();
        second["id"] = json!("second");
        request["params"]["questions"]
            .as_array_mut()
            .unwrap()
            .push(second);
        assert!(answer(&request, json!({"answers":{"choice":["Small change"]}})).is_err());
        assert_eq!(
            answer(&request, json!({"answers":{}})).unwrap(),
            json!({"answers":{}})
        );
        request["params"]["questions"][0]["id"] = json!("__proto__");
        assert!(normalize(&request).is_err());
    }

    #[test]
    fn form_string_lengths_count_unicode_characters() {
        let request = form();
        assert!(answer(
            &request,
            json!({"action":"accept","content":{"name":"é界","count":2,"region":"west"}})
        )
        .is_ok());
        assert!(answer(
            &request,
            json!({"action":"accept","content":{"name":"界","count":2,"region":"west"}})
        )
        .is_err());
    }
}
