# KiCad Library Loader

Downloadsフォルダを監視し、ダウンロードしたKiCadライブラリを指定先へ自動的に取り込むWindows向けCLIです。

## 対応するデータ

- `.kicad_sym`（シンボルライブラリ）
- `.kicad_mod`（フットプリント）
- `.kicad_wks`（ワークシート）
- `.pretty`（フットプリントライブラリフォルダ）
- `.zip`（展開して中身を取り込み）

取り込み先に`sym-lib-table`または`fp-lib-table`がある場合、存在する相対`uri`を絶対パスへ修正します。`${...}`で始まるKiCad環境変数と、既に絶対パスになっているURIは変更しません。

## 使い方

```powershell
cargo run --release
```

既定では`%USERPROFILE%\Downloads`を監視し、`%USERPROFILE%\Documents\KiCad\libraries`へ取り込みます。起動時に既存ファイルも一度処理します。

一度だけ処理して終了する場合:

```powershell
cargo run --release -- --once
```

監視元・取り込み先を変更する場合:

```powershell
cargo run --release -- `
  --source "D:\Downloads" `
  --destination "D:\KiCad\libraries"
```

`KICAD_USER_LIB_DIR`環境変数を設定すると、`--destination`を省略したときの取り込み先を変更できます。

KiCad側では、取り込み先のライブラリテーブルを「Preferences > Manage Symbol Libraries」または「Manage Footprint Libraries」から追加してください。ライブラリを再配置した場合は、ローダーを再実行するとテーブルの相対パスが修正されます。
