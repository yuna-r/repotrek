pub fn sanitize_user_input(input: &str) -> String {
    let forbidden = ["ignore previous instructions", "reveal secrets", "override system prompt"];
    let mut clean = input.to_string();
    for bad in forbidden {
        if clean.to_lowercase().contains(bad) {
            clean = clean.replace(bad, "[BLOCKED_PROMPT_INJECTION_PATTERN]");
        }
    }
    clean
}
