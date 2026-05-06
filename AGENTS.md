# kagent Project Policy

## Language

- 対話は日本語。
- 変数名と識別子は英語。
- コメントは必要最小限の日本語。

## Product Source

- `PLAN.md` を kagent の企画・要求の一次情報として扱う。
- 実装判断が `PLAN.md` と衝突する場合は、作業前に `.codex/plans/**` に判断理由を残す。

## Planning

- 非自明な調査、設計、実装、AI-DLC/Codex config/hooks/skills 変更では、作業前に `.codex/plans/YYYYMMDDHH-{planname}.md` を作成または更新する。
- plan には最低限、original request、workflow axes、target root、allowed/forbidden paths、phases/checkpoints、outputs、test plan、approval gates、rollback/status を含める。
- `.codex/plans/**` は local working state として扱い、durable な方針は `AGENTS.md`、ソース、テスト、ドキュメントに反映する。

## AI-DLC

- `workspace.yaml` がない間は AI-DLC task workspace ではない。
- 初回 commit と task branch が明確になるまでは、`ai-dlc ensure-context` による Codex user-local fallback context を使う。
- full AI-DLC workspace 化、commit、push、root-export、cleanup は明示承認ゲートとする。

## Rust

- Rust workspace は `crates/**` に分割する。
- `kagent-core` に model / event / lens-facing view data を置く。
- `kagent-ui` に TUI view state / widget-facing model を置く。
- `kagent-kitty`、`kagent-sango`、`kagent-git`、`kagent-agent` は外部 provider adapter 境界として扱う。
- `kagent-cli` は command entry point として薄く保つ。
- UI は `println!` と手作業 padding で作らず、Ratatui-style の layout / widget / state 分離で実装する。

## Verification

- Rust 変更後は最低限 `cargo fmt --check` と `cargo test` を実行する。
- AI-DLC/Codex config/hooks 変更後は、関連する `ai-dlc doctor`、context/status 系コマンド、実運用パスを確認する。

## Safety

- `git reset --hard`、`git clean`、`git push`、`git worktree remove`、破壊的 `rm` は明示承認なしで実行しない。
- user または他者の既存変更を revert しない。
- secrets、tokens、credentials を出力・保存しない。
