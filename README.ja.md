# slack-cli

> Coding agent とシェル自動化のための Rust 製 Slack CLI

`slack-cli` は **coding agent** (Claude Code / Codex / Cursor など) とシェル
自動化のために作られたコマンドラインの Slack クライアントです。`noun → verb`
のコマンド構造を採用し、pipe 検出時は安定した JSON を吐き出すので、agent が
スクレイピングなしで結果を扱えます。

[English version](./README.md)

## ステータス

**プレリリース**。API と JSON 出力スキーマ (`slack-cli/v0`) はまだ安定しません。
CI 等で使う場合はタグ付きリリースを pin してください。

## 特徴

- `noun → verb` コマンド階層: `slack-cli channel list`, `slack-cli message send` …
- **TTY 自動検出**: 対話時はテーブル、pipe 時は構造化 JSON
- **ストリームの厳格分離**: stdout はデータ、stderr はログ・進捗・エラー
- **JSON envelope** に `"schema": "slack-cli/v0"` を含み、agent が出力スキーマを pin 可能
- **`--dry-run`**: 書き込み/管理系コマンドすべてに対応。Slack を呼ばずに *これから呼ぶ予定の API リクエスト* を JSON で返す
- **`--yes`**: 非対話環境での破壊的操作には必須
- **冪等な投稿**: `chat.postMessage` には UUID v4 の `client_msg_id` が付くので、agent のリトライが安全
- **トークン保存**: OS キーチェーン（デフォルト）、設定ファイル (Unix では `0600`)、環境変数のいずれかを workspace ごとに選択
- `rustls` による単一静的バイナリ。OpenSSL 不要

## インストール

> プレリリース段階です。Linux musl / macOS / Windows（x86_64 + aarch64）の
> ビルド済みバイナリは最初のタグ後に GitHub Releases に公開予定。

### ソースから（[`mise`][mise] 推奨）

```sh
git clone https://github.com/<owner>/slack-cli && cd slack-cli
mise install
cargo install --path .
```

## クイックスタート

```sh
# 1. workspace を設定（トークンは既定で OS キーチェーンに保存）
slack-cli auth init

# 2. トークンの動作確認
slack-cli auth whoami --json

# 3. メッセージ送信（まず dry-run）
slack-cli message send '#general' --text 'hello from slack-cli' --dry-run
slack-cli message send '#general' --text 'hello from slack-cli'

# 4. 長文を stdin から流し込む
echo "$(cat report.md)" | slack-cli message send '#general' --text -
```

## コマンド一覧

| グループ     | コマンド                                                 |
|-------------|----------------------------------------------------------|
| `auth`      | `init`, `whoami`, `list`, `logout`                       |
| `channel`   | `list`, `view`, `create`, `archive`, `invite`, `leave`   |
| `message`   | `list`, `view`, `send`, `reply`, `update`, `delete`, `react` |
| `search`    | `messages`, `files`                                      |
| `user`      | `list`, `view`                                           |
| `file`      | `list`, `upload`, `download`                             |
| `completion`| `bash`, `zsh`, `fish`, `powershell`, `elvish`           |

詳しいフラグは `slack-cli <group> --help` を参照。各グループは共通のグローバル
フラグを持ちます（`--json`, `--workspace`, `--config`, `-v`, `--quiet`,
`--no-color`, `--yes`, `--max-results`, `--all`, `--max-pages`, `--dry-run`）。

## JSON 出力スキーマ

すべての JSON 出力は安定した envelope に包まれます:

```jsonc
{
  "ok": true,
  "schema": "slack-cli/v0",
  "data": { /* コマンド固有のデータ */ },
  "request_id": "req_..."
}
```

エラーは **stderr** に同形で出ます:

```jsonc
{
  "ok": false,
  "schema": "slack-cli/v0",
  "error": {
    "code": "channel_not_found",
    "message": "channel '#nonexistent' not found",
    "hint": { "action": "run", "command": "slack-cli channel list" },
    "exit_code": 5,
    "details": { "kind": "channel", "name": "#nonexistent" },
    "request_id": "req_..."
  }
}
```

リスト系は常に items + cursor の形:

```jsonc
{ "data": { "items": [...], "next_cursor": "..." } }
```

## 終了コード

| コード | 意味                                       |
|-------:|--------------------------------------------|
|      0 | 成功                                       |
|      1 | 汎用失敗（ネットワーク、想定外 I/O）       |
|      2 | 設定エラー                                 |
|      3 | 認証失敗                                   |
|      4 | レート制限                                 |
|      5 | リソース未存在 / 識別子曖昧                |
|      6 | トークン種別不一致（user vs bot）          |
|     64 | 引数不正（sysexits `EX_USAGE`）            |
|    130 | 中断（SIGINT）                             |

## 設定

設定ファイルのパスは [`etcetera`][etcetera] が解決します:

- macOS: `~/Library/Application Support/slack-cli/config.toml`
- Linux: `$XDG_CONFIG_HOME/slack-cli/config.toml`（既定: `~/.config/slack-cli/config.toml`）
- Windows: `%APPDATA%\slack-cli\config.toml`

`-c, --config <PATH>` または `SLACK_CLI_CONFIG` で上書き可能。

## セキュリティ

- `--token` フラグは提供しません（`ps` から見えるため）。代わりに
  `--token-stdin` / `SLACK_CLI_TOKEN` 環境変数 / OS キーチェーンを使ってください。
- `Authorization` ヘッダは sensitive マーク済みで、ログに漏れません。
- リリースビルドは `panic = "abort"` でメモリダンプを抑制します。
- 脆弱性報告は [`SECURITY.md`](./SECURITY.md) を参照（準備中）。

## 開発

```sh
mise install         # toolchain と cargo-nextest / insta / deny / git-cliff をインストール
mise run test        # fmt --check + clippy -D warnings + nextest + deny
mise run snap        # 未確定の insta スナップショットをレビュー
mise run run -- channel list
```

## ライセンス

MIT OR Apache-2.0 のデュアルライセンス。

[mise]: https://mise.jdx.dev
[etcetera]: https://crates.io/crates/etcetera
