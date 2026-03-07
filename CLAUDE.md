# f2viewer

タイル分割表示できる画像スライダー。Rust + egui (eframe) で実装。

## 機能

- 縦または横に複数回、窓を分割できる（二分木構造）
- それぞれの窓で画像の表示間隔を指定可能（0.5〜60秒）
- 画像は指定ディレクトリ以下のものをランダム表示
- セパレーターのドラッグで分割比率を調整
- 右クリックのコンテキストメニューで操作

## プロジェクト構成

```
src/
  main.rs           -- エントリポイント
  app.rs            -- アプリ状態、タイマー更新、アクション処理
  split_tree.rs     -- 二分木データモデル（Split/Leaf）
  pane.rs           -- 画像ペインのデータ
  image_loader.rs   -- ディレクトリスキャン、ランダム選択、テクスチャ読込
  ui/
    mod.rs
    tree_ui.rs      -- 再帰的レイアウト描画、セパレータードラッグ
    pane_ui.rs      -- 各ペインの画像表示・コンテキストメニュー
    controls.rs     -- PaneAction列挙型
```

## Development Workflow

```bash
cargo build   # ビルド
cargo run     # 実行
make          # リリースビルド＆実行
```

## Coding Principles

- **KISS**: 最小限の実装で目的を達成する。不要な抽象化を作らない
- **DRY**: 共通処理は関数に抽出。パターンが3回繰り返されたら抽象化を検討
- **Security**: ファイル削除はトラッシュ経由（`trash` crate）、破壊的操作は確認ダイアログ、パストラバーサル防止

## 技術メモ

- フォント: HackGen35ConsoleNF-Regular.ttf（日本語表示用、`dirs::font_dir()` またはユーザーローカルフォントから読込）
- egui 0.31 / eframe 0.31（glow バックエンド）
- 画像形式: JPEG, PNG, GIF, BMP, WebP
