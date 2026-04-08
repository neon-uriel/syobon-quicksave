# syobon-quicksave

しょぼんのアクション (Syobon Action / Cat Mario) 向けのクイックセーブ/ロード DLL。

## 仕組み

- 対象プロセスに DLL をインジェクトし、ポーリングスレッドでホットキーを監視
- ゲームの固定メモリ領域（ASLR なし・ImageBase `0x400000`）を直接読み書き
- セーブ対象: `0x5A1000–0x5A2000`（4 KB）+ `0x87A000–0x890000`（88 KB）

## ファイル構成

```
syobon_quicksave.dll   インジェクトする DLL 本体
inject.exe             DLL インジェクター
config.exe             キー設定 GUI
launch.bat             ゲーム起動 + インジェクトを一発で行う
quicksave.cfg          キー設定ファイル（config.exe が自動生成）
```

## 使い方

1. `launch.bat` をダブルクリック（ゲーム起動 + インジェクトを自動実行）
2. ビープ音が鳴れば成功

| キー | 動作 |
|------|------|
| F5（デフォルト） | クイックセーブ（高い音） |
| F9（デフォルト） | クイックロード（低い音） |

キーは `config.exe` で変更できる。設定は `quicksave.cfg` に保存され次回以降自動で読み込まれる。

## ビルド

```bash
# i686 ターゲットが必要
rustup target add i686-pc-windows-msvc

cargo build --release
```

成果物は `target/i686-pc-windows-msvc/release/` に生成される。

## 動作確認済み環境

- しょぼんのアクション（x86 32bit PE、MSVC 2005 ビルド）
- Windows 11

## 静的解析メモ

| アドレス | 内容 |
|----------|------|
| `0x402850` | WinMain |
| `0x407B90` | ゲームメインループ（毎フレーム） |
| `0x4088F1` | 物理演算・位置更新 |
| `0x427B30` | DxLib ScreenFlip ラッパー |
| `0x5A1274` | game_state（1=プレイ中） |
| `0x5A1290` | 残機数 |
| `0x8806F4` | プレイヤー X 座標 |
| `0x88AD7C` | プレイヤー Y 座標 |
| `0x87A544` | プレイヤー X 速度 |
| `0x880B24` | プレイヤー Y 速度 |
