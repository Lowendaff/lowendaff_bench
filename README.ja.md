# lowendaff-bench

[English](README.md) | [简体中文](README.zh-CN.md) | **日本語**

Rust で実装された、設定ファイル不要・単一バイナリの Linux サーバー向け総合ベンチマーク・診断ツールです。専用物理サーバー（ベアメタル）、仮想マシン（KVM / Xen / Cloud VM など）、コンテナ（Docker / LXC / OpenVZ など）に対応しています。

- **単一の静的リンクバイナリ・全 12 プラグイン・設定ファイル不要**：`x86_64` および `aarch64` 向けの `musl` 静的バイナリ 1 つに全 12 項目の検査プラグインを内蔵。すべての選択・フィルタは型安全なコマンドライン列挙オプションで完結します。
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

# 3. 日常の簡易チェック：公開スコア送信を伴う Geekbench 7 や経路追跡をスキップ
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

# 8. 列挙オプションを使って国別サーバー・経路追跡グループ・配信チェック地域を絞り込む
sudo ./lowendaff-bench --only network.iperf3 --countries JP,DE
sudo ./lowendaff-bench --only network.trace --trace-groups services,china
sudo ./lowendaff-bench --only ip.media --media-groups global,asia

# 9. 外向き TCP 25 番ポートが制限されているクラウド環境でメール疎通確認を除外して IP 評価を実行
sudo ./lowendaff-bench --only ip.quality --no-mail

# 10. SOCKS5/HTTP プロキシ経由でツールを取得し、グローバル IP 完全表示・詳細ログを有効化
#     （注：環境変数は sudo の後ろに記述します。IP 判定や配信チェック自体は常に直接通信します）
sudo ALL_PROXY=socks5h://127.0.0.1:1080 ./lowendaff-bench --download-connections 16 --show-ip -v
```

---

## コマンドライン引数・列挙値リファレンス

### オプション一覧

| オプション | 引数形式 | 既定値 | 説明 |
|---|---|---|---|
| `--only` | `<ITEM,...>` | *全項目* | 指定したグループまたはプラグイン ID のみを実行（カンマ区切りまたは複数回指定） |
| `--skip` | `<ITEM,...>` | *なし* | 指定したグループまたは個別プラグイン ID をスキップ（`--only` の結果から除外） |
| `-o, --output` | `<PATH>` | `stdout` | 実行開始前に `<PATH>` を作成し、標準出力の代わりにファイルへレポートを出力（作成不可時は終了コード `2` で即時終了） |
| `-f, --format` | `<FORMAT>` | `text` | 出力形式（`text`、`markdown` / `md`、`json`）。ファイル出力時や `markdown` / `json` では端末幅での折り返しを行いません |
| `--work-dir` | `<DIR>` | `.` | 一時作業ディレクトリの作成先（終了時に自動削除。`tmpfs` や `noexec` の多い `/tmp` は避けてください） |
| `--countries` | `<CODE,...>` | *既定の 5 拠点* | `network.iperf3` の測定対象サーバーを 2 文字の ISO 3166 国コード（大文字小文字不問）または `all` で選択 |
| `--trace-groups` | `<GROUP,...>` | *キャッシュ + 既定拠点* | `network.trace` の追跡対象グループを列挙値で選択（カンマ区切り） |
| `--media-groups` | `<GROUP,...>` | `all` | `ip.media` の配信判定対象リージョンを列挙値で選択（カンマ区切り） |
| `--no-dnsbl` | *フラグ* | `false` | `ip.quality` 内の 25 ゾーン DNSBL ブラックリスト照会をスキップ |
| `--no-mail` | *フラグ* | `false` | `ip.quality` 内の外向き TCP 25 番ポート（MX）疎通確認をスキップ |
| `--download-connections` | `<1..=256>` | `32` | `2 MiB` 以上のファイル取得時の最大並列レンジ接続数（レジュームおよび最大 4 回リトライ対応） |
| `--show-ip` | *フラグ* | `false` | レポート内に自身のグローバル IPv4 / IPv6 アドレスと逆引きホスト名をマスクせず表示 |
| `-v, --verbose` | *フラグ* | `false` | ダウンロード詳細や各ツールの実行コマンド・終了コードをログ出力（機密リンク・シリアル番号・自身の IP は除外） |
| `--list` | *フラグ* | `false` | 全検査項目と対象プラットフォーム・使用ツールをグループ順に表示して終了（`root` 不要） |
| `-h, --help` | *フラグ* | — | ヘルプメッセージを表示して終了 |
| `-V, --version` | *フラグ* | — | バージョン情報を表示して終了 |

### パラメータ列挙値（Enum）一覧

| パラメータ | 指定可能な列挙値 | 内容の説明 |
|---|---|---|
| `--only` / `--skip` *（グループ）* | `cpu` | CPU 検査 2 項目：`cpu.sysbench`、`cpu.geekbench` |
| | `memory` | メモリ検査 4 項目：`memory.sysbench`、`memory.stream`、`memory.latency`、`memory.numa` |
| | `disk` | ディスク検査 2 項目：`disk.health`、`disk.fio` |
| | `network` | ネットワーク検査 2 項目：`network.iperf3`、`network.trace` |
| | `ip` | IP 検査 2 項目：`ip.quality`、`ip.media` |
| `--only` / `--skip` *（個別 ID）* | `disk.health`, `cpu.sysbench`, `memory.sysbench`, `memory.stream`, `memory.latency`, `memory.numa`, `disk.fio`, `network.iperf3`, `network.trace`, `ip.quality`, `ip.media`, `cpu.geekbench` | 個別の検査プラグイン単位で選択・除外（実行順は維持されます） |
| `-f, --format` | `text` | 端末向け整形済みプレーンテキスト（TTY 出力時は端末幅に合わせて表を折り返します） |
| | `markdown` *（別名 `md`）* | GitHub Flavored Markdown（見出し・箇条書き・数値右寄せパイプ表・マスク済み IP の `\*` エスケープ対応） |
| | `json` | 整形済み構造化 JSON（`version`、`environment`、`show_ip`、`system`、`runs`、`not_applicable`、`summary`） |
| `--trace-groups` | `services` | 最寄りの YouTube および Netflix エッジキャッシュサーバーのみを追跡 |
| | `china` | 中国本土キャリア拠点（北京・上海・広州の 163 / CN2 / 169 / 9929 / CMNET / CMIN2） |
| | `international` | 国際 Tier-1 バックボーン、Telegram データセンター、AWS、GCP の各拠点 |
| | `all` | キャッシュサーバー（`services`）とリスト内の全拠点（計 73 ターゲット）を追跡 |
| `--media-groups` | `global` | グローバル配信サービス（Netflix、YouTube Premium、Disney+、Prime Video、Spotify、TikTok など） |
| | `asia` | アジア圏サービス（Bilibili 港澳台、Bahamut、Abema、U-NEXT、TVer、Viu、Coupang Play など） |
| | `europe` | 欧州圏サービス（BBC iPlayer、ITVX、Channel 4、Canal+、ZDF、ARTE、RAI Play など） |
| | `americas` | 北米・中南米圏サービス（Hulu、Max、Peacock、Paramount+、Starz、Crave、Globoplay など） |
| | `oceania` | オセアニア圏サービス（Stan、9Now、7plus、ABC iview、TVNZ+、ThreeNow など） |
| | `all` | 全 5 リージョン・計 68 サービスすべてを判定（`--media-groups` 省略時の既定値） |
| `--countries` | `<ISO-3166-1 2 文字コード>` *（例：`JP`, `US`, `DE`, `SG`, `GB`）* | 指定国の公開 `iperf3` サーバーをすべて使用（大文字・小文字不問） |
| | `all` | `servers/iperf3.json` に登録されている全公開サーバーを使用 |

### プライバシー保護と終了コード

- **デフォルトの IP マスク処理**：`--show-ip` を指定しない場合、`text`・`markdown`・`json` のいずれの形式でも自身のグローバル IPv4（`203.0.*.*`）・IPv6（`2001:db8:*`）および経路追跡内の自ホストホップがマスクされ、`ip.quality` の逆引き `hostname` も非表示（JSON では `null`）となります。
- **終了コード**：
  - `0`：実行完了（個別プラグインのスキップや失敗はレポート内に記録されます）
  - `2`：引数・列挙値の誤り、`--output` 先の作成失敗、または `root` 以外の実行
  - `130`：`Ctrl-C` による中断（完了済みの結果はレポートに出力されます。2 回目の `Ctrl-C` で即時終了）

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

## ビルドとリリース

ツールチェーンは [`rust-toolchain.toml`](rust-toolchain.toml) で固定されています。静的 `musl` バイナリをビルドするには `musl-tools`（`musl-gcc`）が必要です。

```sh
rustup target add x86_64-unknown-linux-musl
cargo build --release --locked --target x86_64-unknown-linux-musl --bin lowendaff-bench
```

ルートの `Cargo.toml` の `[workspace.package] version` と一致する `v*` タグ（例：`v0.1.0`）をプッシュすると、[`.github/workflows/release.yml`](.github/workflows/release.yml) が起動し、`lowendaff-bench-x86_64`・`lowendaff-bench-aarch64` および `SHA256SUMS` を GitHub Releases に公開します。外部ツールの再現ビルド手順や CI 品質ゲート一覧については英語版の [`README.md`](README.md#gates) および [`CONTRIBUTING.md`](CONTRIBUTING.md) を参照してください。

---

## 謝辞（Acknowledgments）

`lowendaff-bench` の設計・測定手法・ツールチェーンは、以下の優れたオープンソースプロジェクト・ベンチマークスイート・公開データ基盤に深く支えられています。開発者・メンテナーの皆様に心より感謝いたします。

- **[YABS (Yet Another Bench Script)](https://github.com/masonr/yet-another-bench-script)**：`disk.fio` の 4 ブロックサイズ混合ランダム I/O パラメータ・速度/IOPS 表記形式、および `network.iperf3` の双方向マルチストリーム TCP 速度測定の設計リファレンス。
- **[IPQuality (`xykt/IPQuality`)](https://github.com/xykt/IPQuality)**：`ip.quality` における複数インテリジェンス源の統合評価、RIR ネイティブ / ブロードキャスト判定、リスク要因集約、DNSBL 照会および TCP 25 番ポートメール疎通検査のリファレンス。
- **[RegionRestrictionCheck (`lmc999/RegionRestrictionCheck`)](https://github.com/lmc999/RegionRestrictionCheck)** および **[MediaUnlockTest (`HsukqiLee/MediaUnlockTest`)](https://github.com/HsukqiLee/MediaUnlockTest)**：`ip.media` における地域別 OTT・動画配信サービスの視聴可否判定のリファレンス。
- **[NextTrace (`nxtrace/NTrace-core`)](https://github.com/nxtrace/NTrace-core)**：`network.trace` を支える ICMP 経路追跡エンジンおよびホップ ASN・位置情報照会サービス。
- **上流ベンチマークツールおよび公開データソース**：[`sysbench`](https://github.com/akopytov/sysbench)（Alexey Kopytov）、[`STREAM`](https://www.cs.virginia.edu/stream/)（John D. McCalpin）、[`multichase`](https://github.com/google/multichase)（Google）、[`smartmontools`](https://www.smartmontools.org/)（`smartctl`）、[`fio`](https://github.com/axboe/fio)（Jens Axboe）、[`iperf3`](https://github.com/esnet/iperf)（ESnet）、[`Geekbench`](https://www.geekbench.com/)（Primate Labs）、[`iperf3serverlist.net`](https://iperf3serverlist.net/)、[`PeeringDB`](https://www.peeringdb.com/)。
