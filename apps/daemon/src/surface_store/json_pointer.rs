// Strict RFC 6901 token parsing and mutation for Surface patch operations.
fn pointer_tokens(path: &str) -> Result<Vec<String>, SurfaceStoreError> {
    if !path.starts_with('/') {
        return Err(SurfaceStoreError::Invalid(
            "node patch path must be a JSON pointer".to_owned(),
        ));
    }
    Ok(path[1..]
        .split('/')
        .map(|token| token.replace("~1", "/").replace("~0", "~"))
        .collect())
}

/// Parses one pointer token as an RFC 6901 array index.
///
/// The grammar is deliberately narrow: `0`, or a digit string with no leading zero. `01`, `+1`,
/// ` 1` and `1.0` are not indices, so a patch that means to address an object key never lands on
/// an array element by accident.
fn pointer_array_index(token: &str) -> Option<usize> {
    if token.is_empty() || !token.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if token.len() > 1 && token.starts_with('0') {
        return None;
    }
    token.parse::<usize>().ok()
}

fn set_json_pointer(target: &mut Value, path: &str, value: Value) -> Result<(), SurfaceStoreError> {
    let tokens = pointer_tokens(path)?;
    let (last, parents) = tokens
        .split_last()
        .ok_or_else(|| SurfaceStoreError::Invalid("cannot replace the entire node".to_owned()))?;
    let mut cursor = target;
    for token in parents {
        // A missing intermediate object is created, which is what makes `/props/a/b` work on a node
        // that has no `a` yet. An intermediate that exists but is the wrong kind is an error: the
        // previous code replaced it with `{}`, so `/props/items/0` on `items: [a, b, c]` quietly
        // turned the array into `{"0": ...}` and dropped the other two elements.
        if cursor.is_null() {
            *cursor = Value::Object(Default::default());
        }
        cursor = match cursor {
            Value::Object(map) => map
                .entry(token.clone())
                .or_insert_with(|| Value::Object(Default::default())),
            Value::Array(items) => {
                let count = items.len();
                let index = pointer_array_index(token).ok_or_else(|| {
                    SurfaceStoreError::Invalid(format!(
                        "node patch path segment `{token}` is not an array index"
                    ))
                })?;
                items.get_mut(index).ok_or_else(|| {
                    SurfaceStoreError::Invalid(format!(
                        "node patch path index {index} is out of range for an array of {count}"
                    ))
                })?
            }
            _ => {
                return Err(SurfaceStoreError::Invalid(format!(
                    "node patch path segment `{token}` traverses a value that is not an object or array"
                )))
            }
        };
    }
    if cursor.is_null() {
        *cursor = Value::Object(Default::default());
    }
    match cursor {
        Value::Object(map) => {
            map.insert(last.clone(), value);
        }
        Value::Array(items) => {
            // `-` is RFC 6901's "the element after the last one", i.e. an append.
            if last == "-" {
                items.push(value);
            } else {
                let count = items.len();
                let index = pointer_array_index(last).ok_or_else(|| {
                    SurfaceStoreError::Invalid(format!(
                        "node patch path segment `{last}` is not an array index"
                    ))
                })?;
                match index.cmp(&count) {
                    std::cmp::Ordering::Less => items[index] = value,
                    std::cmp::Ordering::Equal => items.push(value),
                    std::cmp::Ordering::Greater => {
                        return Err(SurfaceStoreError::Invalid(format!(
                            "node patch path index {index} is out of range for an array of {count}"
                        )))
                    }
                }
            }
        }
        _ => {
            return Err(SurfaceStoreError::Invalid(format!(
                "node patch path segment `{last}` targets a value that is not an object or array"
            )))
        }
    }
    Ok(())
}

fn remove_json_pointer(target: &mut Value, path: &str) -> Result<(), SurfaceStoreError> {
    let tokens = pointer_tokens(path)?;
    let (last, parents) = tokens
        .split_last()
        .ok_or_else(|| SurfaceStoreError::Invalid("cannot remove the entire node".to_owned()))?;
    let mut cursor = target;
    for token in parents {
        // Removing something that is not there stays a no-op, so a repeated patch is harmless.
        // Removing *through* a value of the wrong kind is an error rather than the silent success
        // the previous code returned for every array on the way down.
        cursor = match cursor {
            Value::Object(map) => match map.get_mut(token) {
                Some(next) => next,
                None => return Ok(()),
            },
            Value::Array(items) => {
                let index = pointer_array_index(token).ok_or_else(|| {
                    SurfaceStoreError::Invalid(format!(
                        "node patch path segment `{token}` is not an array index"
                    ))
                })?;
                match items.get_mut(index) {
                    Some(next) => next,
                    None => return Ok(()),
                }
            }
            Value::Null => return Ok(()),
            _ => {
                return Err(SurfaceStoreError::Invalid(format!(
                    "node patch path segment `{token}` traverses a value that is not an object or array"
                )))
            }
        };
    }
    match cursor {
        Value::Object(map) => {
            map.remove(last);
        }
        Value::Array(items) => {
            let index = pointer_array_index(last).ok_or_else(|| {
                SurfaceStoreError::Invalid(format!(
                    "node patch path segment `{last}` is not an array index"
                ))
            })?;
            if index < items.len() {
                items.remove(index);
            }
        }
        Value::Null => {}
        _ => {
            return Err(SurfaceStoreError::Invalid(format!(
                "node patch path segment `{last}` targets a value that is not an object or array"
            )))
        }
    }
    Ok(())
}
