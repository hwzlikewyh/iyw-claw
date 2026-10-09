use super::*;

pub(super) async fn perform_type(
    call: &ActionCall<'_>,
    action: &WindowAction,
    mut args: Value,
) -> Result<RawAct, HelperError> {
    let ActionCall {
        driver,
        mode,
        deliverable,
        ..
    } = *call;
    let WindowAction::Type {
        element,
        text,
        submit,
    } = action
    else {
        return Err(wrong_action());
    };

    put_element(&mut args, element);
    let key = args.clone();
    args["text"] = json!(text);
    deliverable.check()?;
    keys_free(mode, held_modifiers)?;
    let typed = one(driver, "type_text", args, mode, TYPE_TIMEOUT).await?;
    if !*submit {
        return Ok(typed);
    }
    submit_type(call, key, typed).await
}

async fn submit_type(
    call: &ActionCall<'_>,
    mut key: Value,
    typed: RawAct,
) -> Result<RawAct, HelperError> {
    let ActionCall {
        driver,
        mode,
        deliverable,
        ..
    } = *call;
    key["key"] = json!("return");
    // Typing can take a while: the second call is held to the same
    // conditions as the first, at its own moment.
    let pressed = match deliverable
        .check()
        .and_then(|()| keys_free(mode, held_modifiers))
    {
        Ok(()) => one(driver, "press_key", key, mode, ACT_TIMEOUT).await,
        Err(e) => Err(e),
    };
    Ok(match pressed {
        // Both went out: the whole is as sure as its less sure half.
        Ok(pressed) => RawAct {
            effect: weaker(typed.effect, pressed.effect),
            submitted: Some(true),
            ..typed
        },
        // The text went in; return did not. Said as such, and why.
        Err(e) => RawAct {
            submitted: Some(false),
            submit_note: Some(e.message),
            ..typed
        },
    })
}

pub(super) async fn perform_key(
    call: &ActionCall<'_>,
    action: &WindowAction,
    mut args: Value,
) -> Result<RawAct, HelperError> {
    let ActionCall {
        driver,
        mode,
        deliverable,
        ..
    } = *call;
    let platform = Platform::current();
    let WindowAction::Key { element, chord } = action else {
        return Err(wrong_action());
    };

    args["key"] = json!(chord.key.driver_name(platform));
    let modifiers = chord.modifiers.driver_names(platform);
    if !modifiers.is_empty() {
        args["modifiers"] = json!(modifiers);
    }
    if let Some(element) = element {
        put_element(&mut args, element);
    }
    deliverable.check()?;
    keys_free(mode, held_modifiers)?;
    one(driver, "press_key", args, mode, ACT_TIMEOUT).await
}

pub(super) async fn perform_set_value(
    call: &ActionCall<'_>,
    action: &WindowAction,
    mut args: Value,
) -> Result<RawAct, HelperError> {
    let ActionCall {
        driver,
        mode,
        deliverable,
        ..
    } = *call;
    let WindowAction::SetValue { element, value } = action else {
        return Err(wrong_action());
    };

    put_element(&mut args, element);
    args["value"] = json!(value);
    deliverable.check()?;
    one(driver, "set_value", args, mode, ACT_TIMEOUT).await
}
