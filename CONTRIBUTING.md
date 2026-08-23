# Contributing

## Add a UI translation

WhimprFlow's Hub and pill overlay load one JSON file per language from
`ui/src/locales/`. Adding a language needs no code change.

1. Copy `ui/src/locales/en.json` to a new file named after the language's
   [BCP-47](https://en.wikipedia.org/wiki/IETF_language_tag) code, e.g.
   `fr.json` or `pt-BR.json`.
2. Translate every value. Leave the keys (the left side, e.g. `"nav.home"`)
   untouched, and leave placeholders like `{n}`, `{q}`, `{t}` exactly as they
   are, they get replaced with real values at runtime.
3. Set `"_label"` to the language's name in that language (e.g. `"Français"`).
   This is what shows up in the language picker in Settings, it is not a UI
   string itself.
4. Run `pnpm build` inside `ui/` to make sure the file is valid JSON and the
   app still builds.
5. Open a pull request.

The dictation language is independent of the UI language: Whisper detects
the spoken language automatically, so your UI translation only affects menus,
buttons, and labels.
