# Supported Languages

ClearURLs Bot supports 15 languages out of the box. The bot auto-detects the user's language from message content and Telegram client settings, but users can also manually select their preferred language.

## Language List

| Code | Language | Native Name | Flag |
|------|----------|-------------|------|
| `it` | Italian | Italiano | 🇹 |
| `en` | English | English | 🇬🇧 |
| `es` | Spanish | Español | 🇪🇸 |
| `fr` | French | Français | 🇫🇷 |
| `de` | German | Deutsch | 🇩🇪 |
| `pt` | Portuguese | Português | 🇧🇷 |
| `ru` | Russian | Русский | 🇷🇺 |
| `ar` | Arabic | العربية | 🇸🇦 |
| `hi` | Hindi | हिन्दी | 🇮🇳 |
| `zh` | Chinese | 中文 | 🇨🇳 |
| `ja` | Japanese | 日本語 | 🇯🇵 |
| `ko` | Korean | 한국어 | 🇰🇷 |
| `tr` | Turkish | Türkçe | 🇹🇷 |
| `nl` | Dutch | Nederlands | 🇳🇱 |
| `pl` | Polish | Polski | 🇵🇱 |

## How to Change Language

### Via Command

```
/setlang it    # Italian
/setlang en    # English
/setlang es    # Spanish
/setlang fr    # French
/setlang de    # German
/setlang pt    # Portuguese
/setlang ru    # Russian
/setlang ar    # Arabic
/setlang hi    # Hindi
/setlang zh    # Chinese
/setlang ja    # Japanese
/setlang ko    # Korean
/setlang tr    # Turkish
/setlang nl    # Dutch
/setlang pl    # Polish
```

### Via Settings Menu

1. Send `/settings` or tap the "⚙️ Settings" button
2. Tap "🌐 Language"
3. Select your preferred language from the grid

## Auto-Detection

The bot attempts to detect the language in this order:

1. **User's saved preference** — If the user has previously set a language, it's used
2. **Message content** — Uses `whatlang` crate to detect Italian or English from text
3. **Telegram client language** — Falls back to the language code from the Telegram client
4. **Default** — English if nothing else matches

## Translation Guide

### Adding a New Language

1. Open `src/i18n.rs`
2. Add the new field to the `Translations` struct (if needed)
3. Add a new match arm with all 130 translation fields:

```rust
"xx" => Translations {
    cleaning_feedback: "...",
    error_feedback: "...",
    // ... all 130 fields
},
```

4. Add the language label to all existing translation blocks:
```rust
s_language_xx: "🏳️ LanguageName",
```

5. Add the language code to `SUPPORTED_LANGUAGES` in `helpers.rs`:
```rust
pub const SUPPORTED_LANGUAGES: &[&str] = &["it", "en", ..., "xx"];
```

6. Update `language_inline_keyboard` with the new label in the `lang_labels` array.
7. Update the language match arms in `handlers.rs` (`/setlang`, `/language`) and `settings.rs`.
8. Run `cargo fmt`, `cargo clippy`, and `cargo test`.

### Field Categories

| Prefix | Category | Count |
|--------|----------|-------|
| (none) | Core messages (welcome, help, stats) | 17 |
| `d_` | Dashboard strings | 13 |
| `s_` | Settings and UI labels | 60 |
| `rk_` | Reply keyboard buttons | 5 |
| `sec_` | Security alert messages | 10 |
| `err_` | Error messages | 5 |
| `info_` | Info/status messages | 4 |
| (misc) | Other (group_activated, truncated, etc.) | 16 |

**Total: 130 fields per language**
