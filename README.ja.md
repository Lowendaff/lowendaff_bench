# lowendaff-bench

[English](README.md) | [简体中文](README.zh-CN.md) | **日本語**

Rust で実装された Linux サーバー向け総合ベンチマーク・診断ツールです。専用物理サーバー（ベアメタル）、仮想マシン（KVM / Xen / Cloud VM など）、コンテナ（Docker / LXC / OpenVZ など）に対応しています。

- **単一の静的リンクバイナリ・全 12 種類のプラグイン**：`x86_64` および `aarch64` 向けの `musl` 静的バイナリ 1 つに全 12 項目の検査プラグインを内蔵。各プラグインは型安全な結果構造体・個別のタイムアウト・障害分離を備えています。
- **ホスト環境に依存しない再現性**：外部ベンチマークツール（`sysbench`、`STREAM`、`multichase`、`smartctl`、`fio`、`iperf3`、`NextTrace`、`Geekbench 7`）は実行時にオンデマンドで取得され、固定されたファイルサイズと `SHA-256` ハッシュを検証してから実行されます。ホスト側にインストール済みの同名コマンドは一切使用しません。
- **プラットフォーム自動判定とメモリ安全性**：物理サーバー専用項目（`disk.health`）や複数 NUMA ノード必須項目（`memory.numa`）は環境に応じて自動判別されます。メモリ検査は空きメモリの最大 `50%` 以内に収めつつ、CPU 最終レベルキャッシュの `4 倍以上` のワークセットを自動計算します。
- **多彩な出力形式とデフォルトのプライバシー保護**：進捗ログは `stderr` にリアルタイム出力され、最終レポートは整形済みテキスト（`text`）、GitHub Flavored `Markdown`（`markdown` / `md`）、構造化 `JSON`（`json`）から選択可能で、`-o` / `--output` によるファイル直接保存にも対応しています。`--show-ip` を指定しない限り、どの出力形式でも自身のグローバル IP アドレスと逆引きホスト名は自動的にマスクされます。

各検査項目の詳細な測定アルゴリズムや判定ロジックは [`docs/`（中国語ドキュメント）](docs/README.md) を、コード構成や開発規約は [`CONTRIBUTING.md`](CONTRIBUTING.md) を参照してください。

---

## クイックスタート・使用例

[GitHub Releases](https://github.com/Lowendaff/lowendaff_bench/releases) からお使いのアーキテクチャ向けのバイナリ（`lowendaff-bench-x86_64` または `lowendaff-bench-aarch64`）を取得し、`SHA256SUMS` を確認した上で `root` 権限で実行します。

```sh
# 1. 全 12 種類の検査項目・グループ名・対象環境・固定ツールバージョンを一覧表示（root 不要）
./lowendaff-bench --list

# 2. 全項目を実行（stderr に進捗を表示し、完了後に stdout へレポートを出力）
sudo ./lowendaff-bench

# 3. 日常の簡易チェック：公開スコア送信を伴う Geekbench 7 や時間のかかる経路追跡をスキップ
sudo ./lowendaff-bench --skip cpu.geekbench,network.trace

# 4. 特定のグループまたは個別プラグインのみを実行（グループ名とプラグイン ID の混在が可能）
sudo ./lowendaff-bench --only ip
sudo ./lowendaff-bench --only cpu.sysbench,memory.stream,disk.fio

# 5. グループ全体から特定のプラグインだけを除外（例：IP 評価のみ実行し配信サービス判定を除外）
sudo ./lowendaff-bench --only ip --skip ip.media

# 6. レポートを Markdown または JSON 形式でファイルへ直接保存（stdout への重複出力なし）
sudo ./lowendaff-bench -o report.md -f markdown
sudo ./lowendaff-bench -o report.json -f json

# 7. 測定対象のディスク（マウント先）を指定して fio ベンチマークを実行（/tmp は避けてください）
sudo ./lowendaff-bench --only disk.fio --work-dir /mnt/nvme

# 8. 日本とドイツの公開 iperf3 サーバーのみを使って回線速度を測定（通信量を節約）
sudo ./lowendaff-bench --only network.iperf3 --countries JP,DE

# 9. GitHub への接続が不安定な環境で SOCKS5/HTTP プロキシ経由でツールを取得し、並列接続数を指定
#    （注：環境変数は sudo の後ろに記述します。IP 判定や配信チェック自体は常に直接通信します）
sudo ALL_PROXY=socks5h://127.0.0.1:1080 ./lowendaff-bench --download-connections 16

# 10. TOML 設定ファイルを読み込み、グローバル IP をマスクせず完全表示し、詳細ログを有効化
sudo ./lowendaff-bench --config bench.toml --show-ip -v
```

### 設定ファイル（`bench.toml`）の記述例

各プラグインの詳細設定は `[plugins."<id>"]` テーブルで指定できます：

```toml
[plugins."network.iperf3"]
countries = ["JP", "US"]            # ISO 3166 国コード、または ["all"]（--countries が優先）
servers = ["tokyo"]                 # servers/iperf3.json 内のサーバー ID を直接指定（countries より優先）
seconds = 10                        # 片方向あたりの測定秒数（アップロード後にダウンロード）
streams = 8                         # 並列 TCP ストリーム数
rate = 100                          # 1 ストリームあたりの送信上限 Mbit/s（省略時は無制限）

[plugins."network.trace"]
groups = ["china"]                  # services, china, international, all から選択
targets = ["pek-4809", "youtube"]   # 追跡対象 ID を個別指定（groups より優先）
geo = true                          # false にすると各ホップの ASN・地域検索を無効化

[plugins."ip.quality"]
dnsbl = true                        # false にすると 25 ゾーンの DNSBL ブラックリスト照会を省略
mail = true                         # false にすると外向き TCP 25 番ポート（MX）疎通確認を省略

[plugins."ip.media"]
groups = ["global", "asia"]         # global, asia, europe, americas, oceania, all から選択
services = ["netflix", "bahamut"]   # 判定したい配信サービス ID を個別指定（groups より優先）
```

---

## 検査項目の一覧

プラグインの実行前に、まず 30 秒の上限内でカーネル・ファームウェア情報から **システムプロファイル（`[System]`）** を読み取ります。仮想化・コンテナ種別、CPU モデル（ARM コアコードのデコード対応）とキャッシュ構成、メモリ・Swap・cgroup 制限、OS・カーネル・稼働時間・負荷・TCP 輻輳制御、ディスクとファイルシステム、NIC リンク速度と PCIe レーン速度低下の警告、IPv4/IPv6 の直接疎通とグローバル IP に加え、物理サーバーでは DMI/SMBIOS メモリスロット、ECC/EDAC エラー数、BMC/IPMI、ハードウェア RAID カードも表示します（詳細：[`docs/system.md`](docs/system.md)）。

| グループ | プラグイン ID | 項目名 | 対象環境 | 測定内容の概要 | 詳細ロジック |
|---|---|---|---|---|---|
| `disk` | `disk.health` | ディスク健全性 | 物理サーバーのみ | `smartctl` を読み取り専用で実行し、NVMe/SATA/SAS や MegaRAID・3SNIC 配下の物理ディスクの SMART 判定・通電時間・温度・摩耗率・総書込量・不良ブロック・リンクエラーを取得（シリアル番号等は記録しません） | [`docs/disk.md`](docs/disk.md) |
| `cpu` | `cpu.sysbench` | CPU 性能 | 全環境 | `sysbench` 素数計算テスト（上限 `10000`・各 20 秒）を 1 スレッドおよび全利用可能スレッドで実行し、秒間イベント数とマルチスレッド倍率を算出 | [`docs/cpu.md`](docs/cpu.md) |
| `memory` | `memory.sysbench` | メモリ sysbench | 全環境 | 最終キャッシュの 4 倍以上かつ空きメモリの半分以下のバッファを用い、1 スレッドおよび全スレッドでのシーケンシャル読込・書込速度（`MiB/s`）を測定 | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.stream` | メモリ STREAM | 全環境 | 6 段階の配列サイズ（`2M`〜`64M` 要素）から最適なサイズを自動選択し、`Copy`・`Scale`・`Add`・`Triad` の持続メモリ帯域幅（`MB/s`）を測定 | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.latency` | メモリ遅延 | 全環境 | `multichase` のランダムポインタチェイスにより、`L1d`・`L2`・`L3` キャッシュおよびメインメモリのアクセス遅延（`ns`）を階層別に測定 | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.numa` | メモリ NUMA | 利用可能 NUMA ノード `>= 2` | 「CPU ノード × メモリノード」の全組み合わせについて、`multiload` による帯域幅と `multichase` による遅延のマトリクスを測定 | [`docs/memory.md`](docs/memory.md) |
| `disk` | `disk.fio` | ディスク性能 | 全環境 | YABS 互換の `libaio` + `O_DIRECT` 50/50 ランダムリード・ライトを `4K`・`64K`・`512K`・`1M` の 4 ブロックサイズで測定（RAM ディスク上では自動スキップ） | [`docs/disk.md`](docs/disk.md) |
| `network` | `network.iperf3` | ネットワーク速度 | 全環境（要通信） | YABS 互換の 8 ストリーム・各 10 秒の双方向 TCP スループットと最小 RTT を IPv4 / IPv6 で測定（既定：LA・NY・バージニア・東京・フランクフルトの 5 拠点） | [`docs/network.md`](docs/network.md) |
| `network` | `network.trace` | 経路追跡 | 全環境（要 `CAP_NET_RAW`） | `cap_net_raw+ep` の最小権限と非特権ユーザー `nobody` で `NextTrace`（ICMP）を実行し、最寄りの YouTube / Netflix キャッシュと国内外 71 拠点へのホップ・Netflix IX 接続点を表示 | [`docs/network.md`](docs/network.md) |
| `ip` | `ip.quality` | IP レピュテーション | 全環境（要グローバル IP） | 9 つの脅威インテリジェンス源（ASN、RIR **ネイティブ / ブロードキャスト**判定、用途分類、不正スコア、Shodan 公開ポート/CVE、7 種のリスク要因）、25 の DNSBL（公開 DNS のポリシー拒否を除外）、8 大メールサービスの TCP 25 番ポート疎通を評価 | [`docs/ip.md`](docs/ip.md) |
| `ip` | `ip.media` | 配信サービス判定 | 全環境（要通信） | プロキシを経由せず、世界 5 地域（`global`・`asia`・`europe`・`americas`・`oceania`）計 68 の動画配信・オンラインサービスへ IPv4 / IPv6 で直接 HTTPS 接続し、視聴可否・リージョン・オリジナル作品限定・WAF ブロックを判定 | [`docs/ip.md`](docs/ip.md) |
| `cpu` | `cpu.geekbench` | Geekbench 7 | `glibc >= 2.35` + IPv4 | 公式 Geekbench 7.1.0 によるシングル / マルチコアスコアと結果 URL を取得。メモリ不足の環境では一時 Swap ファイル（最大 8 GiB）の自動作成と OOM 保護を実施 | [`docs/cpu.md`](docs/cpu.md) |

---

## 主なオプション・プライバシー保護・終了コード

| オプション | 説明 |
|---|---|
| `--only <LIST>` | 指定したグループまたはプラグイン ID のみを実行（カンマ区切りまたは複数回指定） |
| `--skip <LIST>` | 指定したグループまたは個別プラグイン ID をスキップ（例：`--skip cpu.geekbench,ip.media`） |
| `-o, --output <PATH>` | 最終レポートを標準出力ではなく `<PATH>` に直接書き出す（ベンチマーク開始前に書込可否を検証） |
| `-f, --format <FMT>` | 出力形式：`text`（既定）、`markdown`（別名 `md`）、`json`。ファイル出力時や `markdown` / `json` では端末幅での折り返しを行いません |
| `--config <PATH>` | `[plugins."<id>"]` テーブルを含む TOML 設定ファイルを指定 |
| `--work-dir <DIR>` | 一時作業ディレクトリの作成先（終了時に自動削除。`tmpfs` や `noexec` の多い `/tmp` は避けてください） |
| `--countries <CODES>` | `network.iperf3` で使用するサーバーの国コード（例：`JP,DE`）または `all` |
| `--download-connections <N>` | `2 MiB` 以上のファイル取得時の最大並列レンジ接続数（`1..=256`、既定値 `32`） |
| `--show-ip` | レポート内に自身のグローバル IPv4 / IPv6 アドレスと逆引きホスト名をマスクせず表示 |
| `-v, --verbose` | ダウンロード詳細や各ツールの実行コマンド・終了コードをログ出力（機密リンクやシリアル番号は除外） |
| `--list` | 全検査項目と対象プラットフォーム・使用ツールを表示して終了 |

- **終了コード**：
  - `0`：実行完了（個別プラグインのスキップや失敗はレポート内に記録されます）
  - `2`：引数エラー、設定ファイルの不備、`--output` 先の作成失敗、または `root` 以外の実行
  - `130`：`Ctrl-C` による中断（完了済みの結果はレポートに出力されます。2 回目の `Ctrl-C` で即時終了）

---

## ビルドとリリース

ツールチェーンは [`rust-toolchain.toml`](rust-toolchain.toml) で固定されています。静的 `musl` バイナリをビルドするには `musl-tools`（`musl-gcc`）が必要です。

```sh
rustup target add x86_64-unknown-linux-musl
cargo build --release --locked --target x86_64-unknown-linux-musl --bin lowendaff-bench
```

ルートの `Cargo.toml` の `[workspace.package] version` と一致する `v*` タグ（例：`v0.1.0`）をプッシュすると、[`.github/workflows/release.yml`](.github/workflows/release.yml) が起動し、`lowendaff-bench-x86_64`・`lowendaff-bench-aarch64` および `SHA256SUMS` を GitHub Releases に公開します。外部ツールの再現ビルド手順や CI 品質ゲート一覧については英語版の [`README.md`](README.md#gates) および [`CONTRIBUTING.md`](CONTRIBUTING.md) を参照してください。
