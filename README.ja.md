# lowendaff-bench

[English](README.md) | [简体中文](README.zh-CN.md) | **日本語**

`lowendaff-bench` は、Linux サーバーのハードウェア性能・回線速度・IP 品質・動画配信サービスの対応状況をまとめて診断できるオールインワンツールです。専用物理サーバー、VPS・クラウド（KVM / Xen など）、コンテナ（Docker / LXC / OpenVZ など）ですぐに使えます。

### 主な特徴

- **単一ファイルですぐ動く・事前インストールや設定ファイルは不要**：実行ファイルを 1 つダウンロードするだけで動作します（`x86_64` / `aarch64` 対応）。引数なしで実行すると対話型 TUI 画面が開き、任意のオプション（`-a` / `--all` など）を付けると非対話 CLI モードで動作します。
- **どの OS でも同じ基準で公平に比較可能**：測定に使う外部ツール（`sysbench`、`7-Zip`、`UnixBench`、`HPL`、`PassMark`、`fio`、`iperf3`、`NextTrace`、`Geekbench 7` など）は、実行時に固定バージョンを自動取得し、サイズと SHA-256 を検証して動かします。サーバー側の古いパッケージを使わないため、OS が違ってもスコアをそのまま比較できます。
- **環境に合わせて自動調整・小メモリ VPS でも安心**：仮想環境での物理ディスク SMART 診断、シングル CPU での NUMA 測定、ARM64 環境での `x86_64` 専用 CPU 負荷テストなど、非対応の項目やサブスイートは自動でスキップします。メモリや HPL の規模は空きメモリに応じて自動調整され、Geekbench 7 実行時にメモリが足りない場合は一時的な Swap を自動作成してクラッシュを防ぎます。
- **個人情報を自動でマスク・`/tmp` に自動保存**：端末での表形式表示や **Markdown** / **JSON** 出力に対応し、既定で `/tmp/lowendaff-bench-<タイムスタンプ>.<ext>` にもレポートを自動保存します。グローバル IP アドレスやホスト名、シリアル番号は既定で伏せ字になります。

各テストの詳しい計算方法や判定基準については [`docs/`（中国語解説）](docs/README.md) をご覧ください。

---

## かんたん 3 ステップで実行

[GitHub Releases](https://github.com/Lowendaff/lowendaff_bench/releases) からお使いの CPU 向けのファイル（通常の Intel/AMD サーバーは `lowendaff-bench-x86_64`、ARM64 サーバーは `lowendaff-bench-aarch64`）をダウンロードし、実行権限を付けて `sudo`（または `root`）で実行します。

```sh
# x86_64 サーバーの例（ARM64 の場合は lowendaff-bench-aarch64 に置き換えてください）
curl -fLO https://github.com/Lowendaff/lowendaff_bench/releases/latest/download/lowendaff-bench-x86_64
chmod +x lowendaff-bench-x86_64
sudo ./lowendaff-bench-x86_64        # 引数なしで対話型 TUI を起動（非対話で全実行する場合は --all）
```

> 完了時には画面表示に加え、`/tmp/lowendaff-bench-<タイムスタンプ>.<txt|md|json>` にもレポートが自動保存されます。途中で `Ctrl-C` を 1 回押すと安全に中断し、完了済み項目のレポートを出力します。

---

## よく使う実行例

以下はバイナリ名を `lowendaff-bench` にした場合の実用例です。

```sh
# 1. 利用できる全 15 項目のテスト一覧を確認する（root 権限不要）
./lowendaff-bench --list

# 2. 対話型 TUI モードを起動する（引数なしの既定動作：項目・サブ項目の連動切替、設定、進捗・レポート閲覧）
sudo ./lowendaff-bench

# 2b. 非対話 CLI モードですべての項目をまとめて測定する
sudo ./lowendaff-bench --all

# 3. 手早くチェックする：軽量な CPU サブスイートのみ実行し（UnixBench と Geekbench 7 を省略）、経路追跡もスキップ
sudo ./lowendaff-bench --cpu-bench sysbench,7zip,hpl,passmark --skip network.trace

# 4. 契約直後の VPS で IP の評判・動画配信判定、または BGP 上流トポロジー図だけを確認する
sudo ./lowendaff-bench --only ip
sudo ./lowendaff-bench --only network.bgp

# 5. 気になる項目だけを選んで実行し、x86_64 向けの y-cruncher / Prime95 負荷テストも有効化する
sudo ./lowendaff-bench --only cpu.bench,memory.stream,disk.fio --cpu-bench sysbench,7zip,ycruncher,prime95

# 6. カテゴリ全体から特定の項目だけを除外する（例：IP 品質だけ調べて配信チェックは省く）
sudo ./lowendaff-bench --only ip --skip ip.media

# 7. レポートを Markdown（記事・投稿向け）や JSON（集計・スクリプト向け）で保存する
sudo ./lowendaff-bench -o report.md -f markdown
sudo ./lowendaff-bench -o report.json -f json

# 8. 別ドライブ（マウント先）のディスク速度を測る（※メモリ上の /tmp は避けてください）
sudo ./lowendaff-bench --only disk.fio --work-dir /mnt/data

# 9. 地域を絞って測定：日本と米国の速度測定、中国主要キャリア（Taier）速度測定、中国向け経路、アジア圏の配信サービスのみ確認
./lowendaff-bench --taier-list guangdong
sudo ./lowendaff-bench --only network.iperf3 --countries JP,US
sudo ./lowendaff-bench --only network.taier --taier beijing:cu,guangdong/shenzhen:ct+cm,jiangsu:ct
sudo ./lowendaff-bench --only network.trace --trace-groups china
sudo ./lowendaff-bench --only ip.media --media-groups global,asia

# 10. 25 番メールポートが制限されているクラウドで、メール疎通確認を除いて IP 品質を調べる
sudo ./lowendaff-bench --only ip.quality --no-mail

# 11. GitHub ミラーまたは SOCKS5 プロキシ経由でツールのダウンロードを高速化し、レポートに完全な IP を表示する
#     （※環境変数のプロキシは無視されます。プロキシはファイル取得のみに使われ、IP 判定・速度・経路・BGP・配信テストは常に直接通信します）
sudo ./lowendaff-bench --all --github-proxy https://ghfast.top/
sudo ./lowendaff-bench --all --socks5-proxy socks5h://127.0.0.1:1080 --download-connections 16 --show-ip
```

---

## コマンドオプションと指定できる値

### オプション一覧

| オプション | 書き方 | 既定の動作 | わかりやすい説明 |
|---|---|---|---|
| *（引数なし）* | — | 対話型 TUI | オプションを付けずに `sudo ./lowendaff-bench` を実行すると対話型ターミナル画面を起動します（親スイッチとサブ項目の双方向連動、パラメータ変更、進捗表示、レポート閲覧） |
| `-a, --all` | 値なし | オフ | 非対話 CLI モードで全 15 項目を実行します（`--only all` と同等） |
| `--only` | `<項目名,...>` | 全項目を実行 | CLI モードで指定したカテゴリ・個別 ID・`all` だけを実行します（カンマ区切り） |
| `--skip` | `<項目名,...>` | スキップなし | 指定したカテゴリ・個別 ID・`all` を除外します（`--all` や `--only` と併用可能） |
| `-o, --output` | `<ファイル>` | 画面 + `/tmp` 保存 | 最終レポートを指定ファイルのみに出力します（未指定時は画面/ビューア表示に加え `/tmp/lowendaff-bench-<タイムスタンプ>.<ext>` に自動保存） |
| `-f, --format` | `<形式>` | `text` | 出力形式を選びます：`text`（テキスト表）、`markdown`（別名 `md`）、`json` |
| `--work-dir` | `<ディレクトリ>` | カレント `.` | 一時ファイルの作成先（終了時に自動削除）。測定したいディスクのパスを指定します（`/tmp` は不可） |
| `--cpu-bench` | `<スイート,...>` | 既定の 6 種 | `cpu.bench` で実行する CPU サブスイートを選びます（`sysbench`、`7zip`、`unixbench`、`hpl`、`passmark`、`ycruncher`、`prime95`、`geekbench`、または `all`） |
| `--countries` | `<国コード,...>` | 既定の 5 拠点 | `network.iperf3` の測定先を 2 文字の国コード（`JP,US,SG` など）または `all` で絞り込みます |
| `--taier` | `<セレクタ,...>` | 北京・上海・広州 | `network.taier` の測定先を `<拠点>[:<キャリア>[+<キャリア>...]]` 形式で個別指定します（例：`beijing:cu,guangdong/shenzhen:ct+cm,jiangsu:ct`） |
| `--taier-list` | `[省名]` | — | `network.taier` の全地域（または指定した省の都市・セレクタ・キャリア数）を表示して終了します（`root` 不要） |
| `--taier-cities` | `<都市/省,...>` | 北京・上海・広州 | `network.taier` の測定先を都市・省名・拠点 ID（`Beijing,Jiangsu` など）または `all` で絞り込みます |
| `--taier-operators` | `<キャリア,...>` | `telecom,unicom,mobile` | `network.taier` で測定する通信キャリアを選びます（`telecom`、`unicom`、`mobile`、`cernet`、`cbn`、`other`、または `all`） |
| `--no-cloudflare-udp` | 値なし | オフ | `network.cloudflare` 内の RFC 5389 STUN UDP パケットロス測定を省略します |
| `--inetspeed-threads` | `<1..=64>` | `4` | `network.inetspeed` のマルチスレッド測定で使用する並列スレッド数 |
| `--inetspeed-endpoint` | `<IP>` | 自動（Dual DoH） | `network.inetspeed` で特定の Apple CDN エッジ IP を固定します |
| `--trace-groups` | `<グループ,...>` | キャッシュ + 既定拠点 | `network.trace` で追跡する宛先グループを選びます |
| `--media-groups` | `<地域,...>` | `all`（全地域） | `ip.media` でチェックする動画配信サービスの地域を選びます |
| `--no-dnsbl` | 値なし | オフ | `ip.quality` 内の 25 カ所のブラックリスト（DNSBL）照会を省略します |
| `--no-mail` | 値なし | オフ | `ip.quality` 内の 25 番メールポート接続テストを省略します |
| `--no-bgp-upload` | 値なし | オフ | `text` 形式での `network.bgp` トポロジー SVG の `imageupload.app` へのアップロードを省略します |
| `--download-connections` | `<1..=256>` | `32` | 測定ツールをダウンロードする際の最大同時接続数 |
| `--socks5-proxy` | `<URL>` | なし | ツール・リスト取得専用の SOCKS5 プロキシ（`socks5://` / `socks5h://` 対応。環境変数は参照せず、測定通信には一切干渉しません） |
| `--github-proxy` | `<URL>` | なし | GitHub 取得専用の URL プレフィックスミラー（例：`https://ghfast.top/` で `https://ghfast.top/https://github.com/...` に変換。`--socks5-proxy` と併用可能） |
| `--show-ip` | 値なし | オフ（伏せ字） | レポート内のグローバル IP アドレスや逆引きホスト名を隠さず表示します |
| `-v, --verbose` | 値なし | オフ | ダウンロード状況や実行コマンドの詳細ログを表示します（トラブル調査用） |
| `--list` | 値なし | — | 全テスト項目・対象環境・ツール一覧を表示して終了します（`root` 不要） |
| `-h, --help` | 値なし | — | 使い方ヘルプを表示します |
| `-V, --version` | 値なし | — | バージョン番号を表示します |

### 各オプションで指定できる値

| オプション | 指定できる値 | 内容 |
|---|---|---|
| `--only` / `--skip` *（カテゴリ）* | `cpu` | 統合 CPU ベンチマークスイート：`cpu.bench` |
| | `memory` | メモリテスト 4 項目：`memory.sysbench`、`memory.stream`、`memory.latency`、`memory.numa` |
| | `disk` | ディスクテスト 2 項目：`disk.health`、`disk.fio` |
| | `network` | ネットワークテスト 6 項目：`network.iperf3`、`network.taier`、`network.cloudflare`、`network.inetspeed`、`network.trace`、`network.bgp` |
| | `ip` | IP テスト 2 項目：`ip.quality`、`ip.media` |
| `--only` / `--skip` *（個別 ID）* | `disk.health`, `memory.sysbench`, `memory.stream`, `memory.latency`, `memory.numa`, `disk.fio`, `network.iperf3`, `network.taier`, `network.cloudflare`, `network.inetspeed`, `network.trace`, `network.bgp`, `ip.quality`, `ip.media`, `cpu.bench` | 個別のテスト項目をピンポイントで選択・除外します |
| `-f, --format` | `text` | ターミナルで読みやすい整形済みテキスト表 |
| | `markdown` *（または `md`）* | ブログ・GitHub・掲示板にそのまま貼り付けられる Markdown 形式 |
| | `json` | スクリプトでの自動集計や保存に便利な JSON 形式 |
| `--cpu-bench` | `sysbench` | 1 スレッドおよび全スレッドでの 20 秒間素数計算 *（既定）* |
| | `7zip` | 7-Zip LZMA2 圧縮・展開 MIPS（1 スレッド / 全スレッド）*（既定）* |
| | `unixbench` | Byte-UnixBench 6.0.1 全 12 種システム総合指数（1 並列 / 全コア並列）*（既定）* |
| | `hpl` | High-Performance Linpack（OpenBLAS 倍精度密行列 LU 分解）GFLOPS *（既定）* |
| | `passmark` | PassMark PerformanceTest Linux CPU Mark 総スコアと 8 種の詳細スコア *（既定・`glibc >= 2.28`）* |
| | `ycruncher` | y-cruncher マルチスレッド円周率 Pi 計算 *（任意有効化・`x86_64` 専用）* |
| | `prime95` | Prime95 (`mprime`) 65 秒間 Small-FFT 安定性ストレステスト *（任意有効化・`x86_64` 専用）* |
| | `geekbench` | 公式 Geekbench 7.1.0 シングル / マルチコアスコアと結果 URL *（既定・`glibc >= 2.35` + IPv4）* |
| | `all` | 上記 8 種の CPU サブスイートすべて（非対応環境の項目は自動スキップ） |
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
| `--taier` | `<拠点>[:<キャリア>[+<キャリア>...]]` *（例：`beijing:cu`, `guangdong/shenzhen:ct+cm`, `jiangsu:ct`, `all:cm`）* | 直轄市・`省/都市`・省単位でキャリア（`ct`/`cu`/`cm`/`edu`/`cbn`/`other`/`all`）を組み合わせて指定 |
| `--taier-cities` | 都市・省名・拠点 ID *（例：`Beijing`, `Jiangsu`, `guangdong-shenzhen`）* | 指定した中国本土の CAICT GlobalSpeed（Taier）拠点で速度を測定（大文字・小文字不問） |
| | `all` | Taier ノード一覧にあるすべての拠点で測定 |
| `--taier-operators` | `telecom`, `unicom`, `mobile`, `cernet`, `cbn`, `other`, または `all` | CAICT GlobalSpeed の通信キャリアを絞り込みます（既定：`telecom,unicom,mobile`） |
| `--socks5-proxy` | `socks5://[user:pass@]host[:port]` | ローカルでホスト名を解決してから SOCKS5 プロキシ経由でファイルをダウンロード |
| | `socks5h://[user:pass@]host[:port]` | リモートの SOCKS5 プロキシ側でホスト名を解決してファイルをダウンロード |
| `--github-proxy` | `https://<mirror>/` *（または `http://<mirror>/`）* | GitHub からのツール・一覧取得 URL の先頭にプレフィックスを付与して直接取得（例：`https://ghfast.top/https://github.com/...`） |

---

## 各テスト項目でわかること

テスト開始前に、まずサーバーの基本構成である **システム概要（`[System]`）** を自動で読み取ります。物理サーバー・VM・コンテナの判別、CPU 型番とキャッシュ容量、メモリと Swap、OS・カーネル・稼働時間・TCP 輻輳制御（BBR など）、ディスク構成、NIC リンク速度（PCIe 帯域低下の警告付き）、IPv4/IPv6 の疎通状況を確認でき、物理サーバーではメモリスロット情報、ECC エラー数、IPMI/BMC、ハードウェア RAID カードも一覧表示します（詳細：[`docs/system.md`](docs/system.md)）。

続いて、以下の最大 15 項目を順番に測定します。

| カテゴリ | 項目 ID・名前 | 対象環境 | このテストでわかること | 詳細解説 |
|---|---|---|---|---|
| `disk` | `disk.health`<br>ディスク健康診断 | 物理サーバーのみ | ハードウェア RAID 配下も含め SMART 情報を読み取り、使用時間・温度・SSD 摩耗度・総書込量・不良セクタの有無から中古ディスクの劣化や故障予兆を確認します | [`docs/disk.md`](docs/disk.md) |
| `memory` | `memory.sysbench`<br>メモリ連続読み書き | 全サーバー | 1 スレッドおよび全スレッドでの連続的なメモリ読み書き速度を測定します | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.stream`<br>メモリ実効帯域幅 | 全サーバー | 定番の STREAM ベンチマークを用い、大量データ処理時の実質的なメモリ帯域幅を測定します | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.latency`<br>キャッシュ・メモリ遅延 | 全サーバー | CPU の L1 / L2 / L3 キャッシュとメインメモリのアクセス遅延（ナノ秒）を階層ごとに測定します | [`docs/memory.md`](docs/memory.md) |
| `memory` | `memory.numa`<br>マルチ CPU メモリ速度 | 複数 NUMA 搭載機 | デュアル CPU などの環境で、手元のメモリと別ソケット側のメモリの速度差・遅延差を測定します | [`docs/memory.md`](docs/memory.md) |
| `disk` | `disk.fio`<br>ディスク読み書き速度 | 全サーバー | YABS 互換条件（ランダム読込 50% / 書込 50%）で `4K`・`64K`・`512K`・`1M` の速度と IOPS を測定します。DB 用途は `4K`、大容量転送は `512K`/`1M` が目安になります | [`docs/disk.md`](docs/disk.md) |
| `network` | `network.iperf3`<br>グローバル回線速度 | 通信可能な環境 | 既定でロサンゼルス・ニューヨーク・バージニア・東京・フランクフルトの公開サーバーと接続し、IPv4 / IPv6 の上り・下り速度と遅延を測定します | [`docs/network.md`](docs/network.md) |
| `network` | `network.taier`<br>中国キャリア回線速度 | IPv4 通信可能な環境 | 純 Rust 実装による中国 CAICT GlobalSpeed（泰爾測速）拠点とのマルチストリーム HTTP/1.1 上り・下り速度測定を行います（既定：北京・上海・広州 × 電信・聯通・移動） | [`docs/network.md`](docs/network.md) |
| `network` | `network.cloudflare`<br>Cloudflare 回線速度 | 通信可能な環境 | 純 Rust 実装による `speed.cloudflare.com` デュアルスタック上り・下り速度、無負荷・高負荷時遅延、Bufferbloat / 安定性評価、および STUN UDP パケットロス・MOS 測定を行います | [`docs/network.md`](docs/network.md) |
| `network` | `network.inetspeed`<br>Apple CDN 回線速度 | 通信可能な環境 | 純 Rust 実装による `mensura.cdn-apple.com` 速度測定。Dual DoH で最小 RTT エッジ IP を自動選択・固定し、シングルスレッドとマルチスレッドの上り・下り速度を測定します | [`docs/network.md`](docs/network.md) |
| `network` | `network.trace`<br>ネットワーク経路追跡 | Raw ソケット対応環境 | YouTube / Netflix キャッシュや中国主要キャリア、海外バックボーンまでの経路を一跳びずつ追跡し、直通か迂回ルートかを可視化します | [`docs/network.md`](docs/network.md) |
| `network` | `network.bgp`<br>BGP 経路トポロジー | グローバル IP 保有機 | 世界の BGP 収集拠点から広告プレフィックス・RPKI 状態・直接の上流キャリアを取得し、`text` 形式では限定公開の `imageupload.app` SVG トポロジー URL、Markdown では Mermaid 図、JSON ではスタンドアロン SVG コードを出力します | [`docs/network.md`](docs/network.md) |
| `ip` | `ip.quality`<br>IP 品質・リスク診断 | グローバル IP 保有機 | 9 つの評価データベースを照合し、**ネイティブ IP / ブロードキャスト IP** の判別、データセンター / 家庭用回線の区分、不正リスク、公開ポート、25 種のブラックリスト、25 番メールポートの疎通を診断します | [`docs/ip.md`](docs/ip.md) |
| `ip` | `ip.media`<br>動画配信サービス判定 | 通信可能な環境 | 世界 5 地域・計 68 の動画配信・オンラインサービス（Netflix、Disney+、YouTube、Abema、U-NEXT、TVer など）にどこまで対応しているかと判定地域を表示します | [`docs/ip.md`](docs/ip.md) |
| `cpu` | `cpu.bench`<br>CPU 統合ベンチマーク | 全サーバー *（スイート別判定）* | 8 種の選択式 CPU サブスイート（`--cpu-bench`）を実行：既定の `sysbench`・`7zip`・`unixbench`・`hpl`・`passmark`・`geekbench` に加え、`x86_64` 向けの `ycruncher` や `prime95` 負荷テストにも対応します | [`docs/cpu.md`](docs/cpu.md) |

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
- **[peer.as (`Archeb/peer.as`)](https://github.com/Archeb/peer.as)**：`network.bgp` における AS パスの階層レイアウト・Tier-1 切り詰め・Prepend 折りたたみおよび SVG 描画の参考。
- **[speedtaier (`transflo/speedtaier`)](https://github.com/transflo/speedtaier)**：`network.taier` における CAICT GlobalSpeed（`dovalid`）認証・マルチストリーム HTTP/1.1 速度測定および中国本土キャリア拠点一覧の参考。
- **[cloudflare-speed-cli (`kavehtehrani/cloudflare-speed-cli`)](https://github.com/kavehtehrani/cloudflare-speed-cli)**：`network.cloudflare` における定常 HTTPS スループット、高負荷時遅延、Waveform Bufferbloat / 安定性評価、および STUN UDP パケットロス・MOS 測定の参考。
- **[iNetSpeed-CLI (`nxtrace/iNetSpeed-CLI`)](https://github.com/nxtrace/iNetSpeed-CLI)**：`network.inetspeed` における Dual DoH エンドポイント探索・IP ピン留めおよび `mensura.cdn-apple.com` 速度測定プロトコルの参考。
- **上流ベンチマークツール・公開データソース**：[`sysbench`](https://github.com/akopytov/sysbench)（Alexey Kopytov）、[`7-Zip`](https://www.7-zip.org/)（Igor Pavlov）、[`Byte-UnixBench`](https://github.com/kdlucas/byte-unixbench）、[`HPL`](https://www.netlib.org/benchmark/hpl/) および [`OpenBLAS`](https://www.openblas.net/)、[`PassMark PerformanceTest`](https://www.passmark.com/products/pt_linux/)、[`y-cruncher`](http://www.numberworld.org/y-cruncher/)（Alexander J. Yee）、[`Prime95 / GIMPS`](https://www.mersenne.org/)、[`Geekbench`](https://www.geekbench.com/)（Primate Labs）、[`STREAM`](https://www.cs.virginia.edu/stream/)（John D. McCalpin）、[`multichase`](https://github.com/google/multichase)（Google）、[`smartmontools`](https://www.smartmontools.org/)（`smartctl`）、[`fio`](https://github.com/axboe/fio)（Jens Axboe）、[`iperf3`](https://github.com/esnet/iperf)（ESnet）、[`RIPEstat`](https://stat.ripe.net/)（RIPE NCC）、[`iperf3serverlist.net`](https://iperf3serverlist.net/)、[`PeeringDB`](https://www.peeringdb.com/)。
