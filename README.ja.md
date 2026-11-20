# lowendaff-bench

[English](README.md) | [简体中文](README.zh-CN.md) | **日本語**

`lowendaff-bench` は、Linux サーバーのハードウェア性能・回線速度・IP 品質・動画配信サービスの対応状況をまとめて診断できるオールインワンツールです。専用物理サーバー、VPS・クラウド（KVM / Xen など）、コンテナ（Docker / LXC / OpenVZ など）ですぐに使えます。

### 主な特徴

- **単一ファイルですぐ動く・事前インストールや設定ファイルは不要**：実行ファイルを 1 つダウンロードするだけで動作します（`x86_64` / `aarch64` 対応）。すべての機能はわかりやすいコマンド引数だけで指定できます。
- **どの OS でも同じ基準で公平に比較可能**：測定に使う外部ツール（`sysbench`、`fio`、`iperf3`、`NextTrace`、`Geekbench 7` など）は、実行時に固定バージョンを自動取得・検証して動かします。サーバー側に入っている古いパッケージを使わないため、OS が違ってもスコアをそのまま比較できます。
- **環境に合わせて自動調整・小メモリ VPS でも安心**：仮想環境での物理ディスク SMART 診断やシングル CPU での NUMA 測定など、非対応の項目は自動でスキップします。メモリテストは空きメモリの半分以下に抑え、Geekbench 7 実行時にメモリが足りない場合は一時的な Swap を自動作成してクラッシュを防ぎます。
- **個人情報を自動でマスク・そのまま共有できるレポート**：端末での表形式表示に加え、**Markdown** や **JSON** ファイルへの保存に対応しています。既定では自身のグローバル IP アドレスやホスト名、シリアル番号を自動で伏せ字にするため、手作業で隠さずにブログや掲示板へ貼り付けられます。

各テストの詳しい計算方法や判定基準については [`docs/`（中国語解説）](docs/README.md) をご覧ください。

---

## かんたん 3 ステップで実行

[GitHub Releases](https://github.com/Lowendaff/lowendaff_bench/releases) からお使いの CPU 向けのファイル（通常の Intel/AMD サーバーは `lowendaff-bench-x86_64`、ARM64 サーバーは `lowendaff-bench-aarch64`）をダウンロードし、実行権限を付けて `sudo`（または `root`）で実行します。

```sh
# x86_64 サーバーの例（ARM64 の場合は lowendaff-bench-aarch64 に置き換えてください）
curl -fLO https://github.com/Lowendaff/lowendaff_bench/releases/latest/download/lowendaff-bench-x86_64
chmod +x lowendaff-bench-x86_64
sudo ./lowendaff-bench-x86_64
```

> 実行中は画面に進捗が表示され、すべて完了すると整形されたレポートが出力されます。途中で `Ctrl-C` を 1 回押すと安全に中断し、完了済みの項目だけのレポートを出力します。

---

## よく使う実行例

以下はバイナリ名を `lowendaff-bench` にした場合の実用例です。

```sh
# 1. 利用できる全 12 項目のテスト一覧を確認する（root 権限不要）
./lowendaff-bench --list

# 2. すべての項目をまとめて測定する（CPU・メモリ・ディスク・回線速度・経路・IP 品質・配信判定）
sudo ./lowendaff-bench

# 3. 手早くチェックする：時間がかかりスコアが公開される Geekbench 7 と経路追跡をスキップ
sudo ./lowendaff-bench --skip cpu.geekbench,network.trace

# 4. 契約直後の VPS で IP の評判と動画配信サービスの視聴可否だけを確認する
sudo ./lowendaff-bench --only ip

# 5. 気になる項目だけを選んで実行する（カテゴリ名と個別 ID は自由に組み合わせ可能）
sudo ./lowendaff-bench --only cpu.sysbench,memory.stream,disk.fio

# 6. カテゴリ全体から特定の項目だけを除外する（例：IP 品質だけ調べて配信チェックは省く）
sudo ./lowendaff-bench --only ip --skip ip.media

# 7. レポートを Markdown（記事・投稿向け）や JSON（集計・スクリプト向け）で保存する
sudo ./lowendaff-bench -o report.md -f markdown
sudo ./lowendaff-bench -o report.json -f json

# 8. 別ドライブ（マウント先）のディスク速度を測る（※メモリ上の /tmp は避けてください）
sudo ./lowendaff-bench --only disk.fio --work-dir /mnt/data

# 9. 地域を絞って測定：日本と米国の速度測定、中国向け経路、アジア圏の配信サービスのみ確認
sudo ./lowendaff-bench --only network.iperf3 --countries JP,US
sudo ./lowendaff-bench --only network.trace --trace-groups china
sudo ./lowendaff-bench --only ip.media --media-groups global,asia

# 10. 25 番メールポートが制限されているクラウドで、メール疎通確認を除いて IP 品質を調べる
sudo ./lowendaff-bench --only ip.quality --no-mail

# 11. プロキシ経由でツールのダウンロードを高速化し、レポートに完全な IP を表示する
#     （※プロキシ変数は sudo の後ろに指定します。IP 判定や速度・配信テストは直接通信します）
sudo ALL_PROXY=socks5h://127.0.0.1:1080 ./lowendaff-bench --download-connections 16 --show-ip
```

---

## コマンドオプションと指定できる値

### オプション一覧

| オプション | 書き方 | 既定の動作 | わかりやすい説明 |
|---|---|---|---|
| `--only` | `<項目名,...>` | 全項目を実行 | 指定したカテゴリまたは個別テストだけを実行します（カンマ区切り） |
| `--skip` | `<項目名,...>` | スキップなし | 指定したカテゴリまたは個別テストを除外します（`--only` と併用可能） |
| `-o, --output` | `<ファイル>` | 画面に表示 | 最終レポートを画面ではなく指定ファイルに保存します |
| `-f, --format` | `<形式>` | `text` | 出力形式を選びます：`text`（テキスト表）、`markdown`（別名 `md`）、`json` |
| `--work-dir` | `<ディレクトリ>` | カレント `.` | 一時ファイルの作成先（終了時に自動削除）。測定したいディスクのパスを指定します（`/tmp` は不可） |
| `--countries` | `<国コード,...>` | 既定の 5 拠点 | `network.iperf3` の測定先を 2 文字の国コード（`JP,US,SG` など）または `all` で絞り込みます |
| `--trace-groups` | `<グループ,...>` | キャッシュ + 既定拠点 | `network.trace` で追跡する宛先グループを選びます |
| `--media-groups` | `<地域,...>` | `all`（全地域） | `ip.media` でチェックする動画配信サービスの地域を選びます |
| `--no-dnsbl` | 値なし | オフ | `ip.quality` 内の 25 カ所のブラックリスト（DNSBL）照会を省略します |
| `--no-mail` | 値なし | オフ | `ip.quality` 内の 25 番メールポート接続テストを省略します |
| `--download-connections` | `<1..=256>` | `32` | 測定ツールをダウンロードする際の最大同時接続数 |
| `--show-ip` | 値なし | オフ（伏せ字） | レポート内のグローバル IP アドレスや逆引きホスト名を隠さず表示します |
| `-v, --verbose` | 値なし | オフ | ダウンロード状況や実行コマンドの詳細ログを表示します（トラブル調査用） |
| `--list` | 値なし | — | 全テスト項目・対象環境・ツール一覧を表示して終了します（`root` 不要） |
| `-h, --help` | 値なし | — | 使い方ヘルプを表示します |
| `-V, --version` | 値なし | — | バージョン番号を表示します |

### 各オプションで指定できる値

| オプション | 指定できる値 | 内容 |
|---|---|---|
| `--only` / `--skip` *（カテゴリ）* | `cpu` | CPU テスト 2 項目：`cpu.sysbench`、`cpu.geekbench` |
| | `memory` | メモリテスト 4 項目：`memory.sysbench`、`memory.stream`、`memory.latency`、`memory.numa` |
| | `disk` | ディスクテスト 2 項目：`disk.health`、`disk.fio` |
| | `network` | ネットワークテスト 2 項目：`network.iperf3`、`network.trace` |
| | `ip` | IP テスト 2 項目：`ip.quality`、`ip.media` |
| `--only` / `--skip` *（個別 ID）* | `disk.health`, `cpu.sysbench`, `memory.sysbench`, `memory.stream`, `memory.latency`, `memory.numa`, `disk.fio`, `network.iperf3`, `network.trace`, `ip.quality`, `ip.media`, `cpu.geekbench` | 個別のテスト項目をピンポイントで選択・除外します |
| `-f, --format` | `text` | ターミナルで読みやすい整形済みテキスト表 |
| | `markdown` *（または `md`）* | ブログ・GitHub・掲示板にそのまま貼り付けられる Markdown 形式 |
| | `json` | スクリプトでの自動集計や保存に便利な JSON 形式 |
| `--trace-groups` | `services` | 最寄りの YouTube / Netflix キャッシュサーバーへの経路のみ |
| | `china` | 中国本土の大手通信キャリア回線（163 / CN2 / 169 / 9929 / CMNET / CMIN2） |
| | `international` | 国際 Tier-1 バックボーン、Telegram DC、AWS、Google Cloud 拠点 |
| | `all` | 上記すべての経路ターゲット（計 73 カ所） |
| `--media-groups` | `global` | 世界共通の主要サービス（Netflix、YouTube Premium、Disney+、Prime Video、Spotify、TikTok など） |
| | `asia` | アジア圏のサービス（Abema、U-NEXT、TVer、Bilibili 港澳台、Bahamut、Viu など） |
| | `europe` | 欧州圏のサービス（BBC iPlayer、ITVX、Channel 4、Canal+、ZDF、ARTE、RAI Play など） |
| | `americas` | 北米・中南米圏のサービス（Hulu、Max、Peacock、Paramount+、Starz、Crave など） |
| | `oceania` | オセアニア圏のサービス（Stan、9Now、7plus、ABC iview、TVNZ+ など） |
| | `all` | 全 5 地域・合計 68 サービスすべてを確認（既定） |
| `--countries` | 2 文字の国コード *（例：`JP`, `US`, `DE`, `SG`, `GB`）* | 指定した国の公開 `iperf3` サーバーすべてで速度を測定（大文字・小文字不問） |
| | `all` | サーバー一覧にあるすべての公開 `iperf3` サーバーで測定 |

---

## 各テスト項目でわかること

テスト開始前に、まずサーバーの基本構成である **システム概要（`[System]`）** を自動で読み取ります。物理サーバー・VM・コンテナの判別、CPU 型番とキャッシュ容量、メモリと Swap、OS・カーネル・稼働時間・TCP 輻輳制御（BBR など）、ディスク構成、NIC リンク速度（PCIe 帯域低下の警告付き）、IPv4/IPv6 の疎通状況を確認でき、物理サーバーではメモリスロット情報、ECC エラー数、IPMI/BMC、ハードウェア RAID カードも一覧表示します（詳細：[`docs/system.md`](docs/system.md)）。

続いて、以下の最大 12 項目を順番に測定します。

| カテゴリ | 項目 ID・名前 | 対象環境 | このテストでわかること | 詳細解説 |
|---|---|---|---|---|
| `disk` | `disk.health`<br>ディスク健康診断 | 物理サーバーのみ | ハードウェア RAID 配下も含め SMART 情報を読み取り、使用時間・温度・SSD 摩耗度・総書込量・不良セクタの有無から中古ディスクの劣化や故障予兆を確認します | [`docs/disk.md`](docs/disk.md) |
| `cpu` | `cpu.sysbench`<br>CPU 基本性能 | 全サーバー | シングルコアおよび全コアで 20 秒間の素数計算を行い、単体コアの速さとマルチコアの伸びを確認します | [`docs/cpu.md`](docs/cpu.md) |
| `memory` | `memory.sysbench`<br>メモリ連続読み書き | 全サーバー | 1 スレッドおよび全スレッドでの連続的なメモリ読み書き速度を測定します | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.stream`<br>メモリ実効帯域幅 | 全サーバー | 定番の STREAM ベンチマークを用い、大量データ処理時の実質的なメモリ帯域幅を測定します | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.latency`<br>キャッシュ・メモリ遅延 | 全サーバー | CPU の L1 / L2 / L3 キャッシュとメインメモリのアクセス遅延（ナノ秒）を階層ごとに測定します | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.numa`<br>マルチ CPU メモリ速度 | 複数 NUMA 搭載機 | デュアル CPU などの環境で、手元のメモリと別ソケット側のメモリの速度差・遅延差を測定します | [`docs/memory.md`](docs/memory.md) |
| `disk` | `disk.fio`<br>ディスク読み書き速度 | 全サーバー | YABS 互換条件（ランダム読込 50% / 書込 50%）で `4K`・`64K`・`512K`・`1M` の速度と IOPS を測定します。DB 用途は `4K`、大容量転送は `512K`/`1M` が目安になります | [`docs/disk.md`](docs/disk.md) |
| `network` | `network.iperf3`<br>グローバル回線速度 | 通信可能な環境 | 既定でロサンゼルス・ニューヨーク・バージニア・東京・フランクフルトの公開サーバーと接続し、IPv4 / IPv6 の上り・下り速度と遅延を測定します | [`docs/network.md`](docs/network.md) |
| `network` | `network.trace`<br>ネットワーク経路追跡 | Raw ソケット対応環境 | YouTube / Netflix キャッシュや中国主要キャリア、海外バックボーンまでの経路を一跳びずつ追跡し、直通か迂回ルートかを可視化します | [`docs/network.md`](docs/network.md) |
| `ip` | `ip.quality`<br>IP 品質・リスク診断 | グローバル IP 保有機 | 9 つの評価データベースを照合し、**ネイティブ IP / ブロードキャスト IP** の判別、データセンター / 家庭用回線の区分、不正リスク、公開ポート、25 種のブラックリスト、25 番メールポートの疎通を診断します | [`docs/ip.md`](docs/ip.md) |
| `ip` | `ip.media`<br>動画配信サービス判定 | 通信可能な環境 | 世界 5 地域・計 68 の動画配信・オンラインサービス（Netflix、Disney+、YouTube、Abema、U-NEXT、TVer など）にどこまで対応しているかと判定地域を表示します | [`docs/ip.md`](docs/ip.md) |
| `cpu` | `cpu.geekbench`<br>Geekbench 7 スコア | `glibc >= 2.35` + IPv4 | 公式の Geekbench 7.1.0 を実行し、他機種と比較しやすいシングル / マルチコアスコアと結果ページの URL を出力します | [`docs/cpu.md`](docs/cpu.md) |

---

## ソースコードからのビルド

通常は [GitHub Releases](https://github.com/Lowendaff/lowendaff_bench/releases) のビルド済みバイナリをそのままご利用いただけます。手元で静的バイナリをビルドする場合は、Rust ツールチェーン（[`rust-toolchain.toml`](rust-toolchain.toml)）と `musl-tools`（`musl-gcc`）を用意して以下を実行してください。

```sh
rustup target add x86_64-unknown-linux-musl
cargo build --release --locked --target x86_64-unknown-linux-musl --bin lowendaff-bench
```

ビルドされた実行ファイルは `target/x86_64-unknown-linux-musl/release/lowendaff-bench` に出力されます。開発への参加方法やコード構成については [`CONTRIBUTING.md`](CONTRIBUTING.md) を参照してください。

---

## 謝辞

`lowendaff-bench` は、多くの優れたオープンソースプロジェクト・ベンチマークツール・公開データ基盤の知見に支えられています。開発者およびメンテナーの皆様に深く感謝いたします。

- **[YABS (Yet Another Bench Script)](https://github.com/masonr/yet-another-bench-script)**：`disk.fio` の 4 ブロックサイズ混合ランダム読み書き条件・表記形式、および `network.iperf3` のマルチストリーム速度測定の参考。
- **[IPQuality (`xykt/IPQuality`)](https://github.com/xykt/IPQuality)**：`ip.quality` の複数ソース照合、ネイティブ / ブロードキャスト IP 判定、DNSBL 照会、25 番ポートメール疎通チェックの参考。
- **[RegionRestrictionCheck (`lmc999/RegionRestrictionCheck`)](https://github.com/lmc999/RegionRestrictionCheck)** および **[MediaUnlockTest (`HsukqiLee/MediaUnlockTest`)](https://github.com/HsukqiLee/MediaUnlockTest)**：`ip.media` の地域別動画配信サービス判定の参考。
- **[NextTrace (`nxtrace/NTrace-core`)](https://github.com/nxtrace/NTrace-core)**：`network.trace` で使用する高速な ICMP 経路追跡エンジンおよび位置情報サービス。
- **上流ベンチマークツール・公開データソース**：[`sysbench`](https://github.com/akopytov/sysbench)（Alexey Kopytov）、[`STREAM`](https://www.cs.virginia.edu/stream/)（John D. McCalpin）、[`multichase`](https://github.com/google/multichase)（Google）、[`smartmontools`](https://www.smartmontools.org/)（`smartctl`）、[`fio`](https://github.com/axboe/fio)（Jens Axboe）、[`iperf3`](https://github.com/esnet/iperf)（ESnet）、[`Geekbench`](https://www.geekbench.com/)（Primate Labs）、[`iperf3serverlist.net`](https://iperf3serverlist.net/)、[`PeeringDB`](https://www.peeringdb.com/)。
