use anyhow::Result;
use async_openai::types::{
    ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs,
};
use async_openai::{Client, config::OpenAIConfig};
use serde::{Deserialize, Serialize};
use tracing::warn;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeResult {
    pub pass: bool,
    pub confidence: f64,
    pub reasoning: String,
}

#[derive(Debug, Clone)]
pub struct JudgeClient {
    client: Client<OpenAIConfig>,
    model: String,
}

impl JudgeClient {
    pub async fn new(api_base: String, api_key: String, model: String) -> Result<Self> {
        let config = OpenAIConfig::new().with_api_base(api_base).with_api_key(api_key);
        let client = Client::with_config(config);

        Ok(Self { client, model })
    }

    pub async fn evaluate(&self, input_url: &str, actual_output: &str, expected_output: &str) -> Result<JudgeResult> {
        let prompt = self.build_judge_prompt(input_url, actual_output, expected_output);

        let request = CreateChatCompletionRequestArgs::default()
            .model(&self.model)
            .messages(vec![
                ChatCompletionRequestSystemMessageArgs::default()
                    .content("You are an expert evaluator for URL sanitization. Your task is to determine if the actual output correctly removes tracking parameters while preserving functional parameters. Respond with a JSON object containing: pass (boolean), confidence (0-1), and reasoning (string).")
                    .build()?
                    .into(),
                ChatCompletionRequestUserMessageArgs::default()
                    .content(prompt)
                    .build()?
                    .into(),
            ])
            .temperature(0.0)
            .max_tokens(500u32)
            .build()?;

        let response = self.client.chat().create(request).await?;

        let content = response
            .choices
            .first()
            .and_then(|c| c.message.content.as_ref())
            .ok_or_else(|| anyhow::anyhow!("Empty response from judge"))?;

        self.parse_judge_response(content)
    }

    fn build_judge_prompt(&self, input_url: &str, actual_output: &str, expected_output: &str) -> String {
        format!(
            r#"Evaluate the following URL sanitization result:

**Input URL:**
```
{}
```

**Expected Output (ground truth):**
```
{}
```

**Actual Output (model prediction):**
```
{}
```

**Evaluation Criteria:**
1. The output must remove ALL tracking parameters (utm_*, fbclid, gclid, msclkid, yclid, _ga, mc_cid, etc.)
2. The output must PRESERVE all functional parameters (ids, search queries, pagination, filters, sorting, authentication tokens, API parameters, etc.)
3. The URL structure (scheme, host, path, fragment) must be preserved
4. Parameter order does not matter

**Tracking Parameters to Remove (non-exhaustive):**
- UTM: utm_source, utm_medium, utm_campaign, utm_term, utm_content
- Social: fbclid, gclid, msclkid, yclid, ttclid, li_fat_id, igshid, pin_id, rdt_cid, twclid, snap_jid
- Email: mc_cid, mc_eid, hubspot_utm, hsCtaTracking, marketo_tok, klaviyo, braze, iterable, customerio
- Analytics: _ga, _gid, _gat, matomo_*, pk_*, s_cid, s_kwcid, adjust_*, branch_*, kochava_*, singular_*, tenjin, mparticle, amplitude_*, mixpanel, heap_*, fullstory, hotjar, luckyorange, mouseflow, clarity, yandex_metrica, baidu
- Affiliate: affiliate_id, aff_id, aff_sub, aff_click, pid, af_id, af_sub1
- Session: session_id, user_id, tracking_id, trace_id, request_id, visitor_id, device_id, client_id, anonymous_id
- Consent: consent, gdpr, ccpa

**Functional Parameters to Preserve (non-exhaustive):**
- IDs: id, product_id, user_id, item_id, order_id, cart_id, checkout_token
- Search: q, query, search, keyword
- Pagination: page, limit, offset, start, rows, page[number], page[size]
- Filtering: filter, filter[status], filter[category], sort, orderBy, sort[-created_at]
- API: format, pretty, include, fields, fields[post], graphql, variables, operationName, callback, _
- Auth: token, api_key, jwt, signature, expires, key, sig, policy, state, nonce, pkce, code, code_challenge
- OAuth/SSO: SAMLRequest, RelayState, ticket, service, session_state, iss
- Payment: payment_intent, payment_method, client_secret, setup_future_usage, transaction_id, amount, currency
- UI: view, layout, theme, locale, currency, lang, debug, variant, test_id
- Download: download, filename, quality
- Embed: embed, width, height, autoplay, muted
- Webhooks: webhook_url, events, redirect_url, cancel_url

**Output Format (JSON only):**
```json
{{
  "pass": true/false,
  "confidence": 0.0-1.0,
  "reasoning": "Detailed explanation of why the output passes or fails"
}}
```"#,
            input_url, expected_output, actual_output
        )
    }

    fn parse_judge_response(&self, content: &str) -> Result<JudgeResult> {
        let content = content.trim();

        // Try to extract JSON from markdown code blocks
        let json_str = if content.starts_with("```json") {
            content
                .strip_prefix("```json")
                .unwrap_or(content)
                .strip_suffix("```")
                .unwrap_or(content)
                .trim()
        } else if content.starts_with("```") {
            content
                .strip_prefix("```")
                .unwrap_or(content)
                .strip_suffix("```")
                .unwrap_or(content)
                .trim()
        } else {
            content
        };

        let result: JudgeResponse = serde_json::from_str(json_str).map_err(|e| {
            warn!("Failed to parse judge response as JSON: {}. Content: {}", e, content);
            anyhow::anyhow!("Failed to parse judge response: {}", e)
        })?;

        Ok(JudgeResult {
            pass: result.pass,
            confidence: result.confidence.clamp(0.0, 1.0),
            reasoning: result.reasoning,
        })
    }
}

#[derive(Deserialize)]
struct JudgeResponse {
    pass: bool,
    confidence: f64,
    reasoning: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_judge_response_json() {
        let client = JudgeClient { client: Client::with_config(OpenAIConfig::new()), model: "test".to_string() };

        let response = r#"{"pass": true, "confidence": 0.95, "reasoning": "Correctly removed tracking params"}"#;
        let result = client.parse_judge_response(response).unwrap();
        assert!(result.pass);
        assert_eq!(result.confidence, 0.95);
    }

    #[test]
    fn test_parse_judge_response_markdown() {
        let client = JudgeClient { client: Client::with_config(OpenAIConfig::new()), model: "test".to_string() };

        let response = r#"```json
{"pass": false, "confidence": 0.9, "reasoning": "Removed functional param 'id'"}
```"#;
        let result = client.parse_judge_response(response).unwrap();
        assert!(!result.pass);
        assert_eq!(result.confidence, 0.9);
    }
}
