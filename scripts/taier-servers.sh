#!/bin/sh
# China carrier speedtest nodes for network.taier (README "China carrier speed"):
# servers/taier.json, which the program downloads at run time, normalized into pure ASCII from
# transflo/speedtaier's decrypted CAICT GlobalSpeed node export (serverlist_decrypted.json).
#
# Usage: scripts/taier-servers.sh check
#            checks servers/taier.json against the rules
#        scripts/taier-servers.sh check-list <file>
#            checks a list in the program's format against the rules that the program checks
#        scripts/taier-servers.sh update <export>
#            writes servers/taier.json anew from serverlist_decrypted.json: groups IPv4 nodes by
#            normalized (province, city), translates Chinese province/city/operator names to ASCII,
#            marks Beijing, Shanghai and Guangzhou as defaults, and prints what changed or
#            "unchanged"
#
# The rules match the program's reader (plugins/network/src/taier/nodes.rs): at most 64 KiB and
# 400 locations of version 1; unique location ids of 1 to 40 characters of a to z, digits and -;
# printable ASCII province and city names of 1 to 40 characters; optional boolean default; and 1 to
# 32 nodes per location, each with operator (telecom, unicom, mobile, cernet, cbn or other), a
# canonical IPv4 address, and an integer port from 1024 to 65535, without duplicate ip:port pairs
# inside a location. Both read the fixtures in plugins/network/tests/fixtures/taier-lists/.
set -eu

LIST=servers/taier.json
LIMIT=65536

fail() {
    echo "taier-servers: $*" >&2
    exit 1
}

# shellcheck disable=SC2016 # jq's variables and interpolations, not the shell's
RULES=$(cat "$(dirname "$0")/list-rules.jq")'
def ascii_printable($longest):
    type == "string" and length >= 1 and length <= $longest and whole("[ -~]+");
def id_rule:
    type == "string" and whole("[a-z0-9-]{1,40}") and . != "all";
def default_rule:
    . == null or type == "boolean";
def operator_rule:
    . == "telecom" or . == "unicom" or . == "mobile"
    or . == "cernet" or . == "cbn" or . == "other";
def port_rule:
    type == "number" and (tojson | whole("[0-9]+")) and . >= 1024 and . <= 65535;
def node_problems:
    [ (if .operator | operator_rule then empty else "operator" end),
      (if .ip | type == "string" and ipv4 then empty else "ip" end),
      (if .port | port_rule then empty else "port" end) ];
def location_problems:
    [ (if .id | id_rule then empty else "id" end),
      (if .province | ascii_printable(40) then empty else "province" end),
      (if .city | ascii_printable(40) then empty else "city" end),
      (if .default | default_rule then empty else "default" end),
      (if .nodes | type == "array" and length >= 1 and length <= 32 then empty else "nodes" end) ]
    + (if .nodes | type == "array" and length >= 1 and length <= 32 then
          [ .nodes | to_entries[]
            | .value | if type == "object" then node_problems else ["node"] end | .[] ]
          + (if ([.nodes[] | select(type == "object") | "\(.ip):\(.port)"]
                 | length != (unique | length))
             then ["duplicate-endpoint"] else [] end)
       else [] end);
def list_problems:
    if type != "object" then ["not an object"]
    elif .version != 1 then ["version \(.version) is not 1"]
    elif .locations | type != "array" then ["no locations"]
    elif .locations | length == 0 then ["empty locations"]
    elif .locations | length > 400 then ["\(.locations | length) locations, more than 400"]
    else
        [ .locations | to_entries[]
          | .key as $index
          | .value | if type == "object" then location_problems else ["not an object"] end
          | .[] | "location \($index + 1): \(.)" ]
        + [ [.locations[] | select(type == "object") | .id | select(type == "string")]
            | group_by(.)[] | select(length > 1) | "id \(.[0]) twice" ]
    end;
'

# Normalization tables use \uXXXX escapes so this script stays pure ASCII.
# shellcheck disable=SC2016
NORMALIZE=$(cat "$(dirname "$0")/list-rules.jq")'
def provinces: {
    "\u5317\u4eac": ["beijing", "Beijing"],
    "\u4e0a\u6d77": ["shanghai", "Shanghai"],
    "\u5929\u6d25": ["tianjin", "Tianjin"],
    "\u91cd\u5e86": ["chongqing", "Chongqing"],
    "\u6cb3\u5317": ["hebei", "Hebei"],
    "\u5c71\u897f": ["shanxi", "Shanxi"],
    "\u5185\u8499\u53e4": ["inner-mongolia", "Inner Mongolia"],
    "\u8fbd\u5b81": ["liaoning", "Liaoning"],
    "\u5409\u6797": ["jilin", "Jilin"],
    "\u9ed1\u9f99\u6c5f": ["heilongjiang", "Heilongjiang"],
    "\u6c5f\u82cf": ["jiangsu", "Jiangsu"],
    "\u6d59\u6c5f": ["zhejiang", "Zhejiang"],
    "\u5b89\u5fbd": ["anhui", "Anhui"],
    "\u798f\u5efa": ["fujian", "Fujian"],
    "\u6c5f\u897f": ["jiangxi", "Jiangxi"],
    "\u5c71\u4e1c": ["shandong", "Shandong"],
    "\u6cb3\u5357": ["henan", "Henan"],
    "\u6e56\u5317": ["hubei", "Hubei"],
    "\u6e56\u5357": ["hunan", "Hunan"],
    "\u5e7f\u4e1c": ["guangdong", "Guangdong"],
    "\u5e7f\u897f": ["guangxi", "Guangxi"],
    "\u6d77\u5357": ["hainan", "Hainan"],
    "\u56db\u5ddd": ["sichuan", "Sichuan"],
    "\u8d35\u5dde": ["guizhou", "Guizhou"],
    "\u4e91\u5357": ["yunnan", "Yunnan"],
    "\u897f\u85cf": ["tibet", "Tibet"],
    "\u9655\u897f": ["shaanxi", "Shaanxi"],
    "\u7518\u8083": ["gansu", "Gansu"],
    "\u9752\u6d77": ["qinghai", "Qinghai"],
    "\u5b81\u590f": ["ningxia", "Ningxia"],
    "\u65b0\u7586": ["xinjiang", "Xinjiang"],
    "\u9999\u6e2f": ["hong-kong", "Hong Kong"],
    "\u6fb3\u95e8": ["macau", "Macau"],
    "\u53f0\u6e7e": ["taiwan", "Taiwan"],
    "\u4e9a\u6d32": ["asia", "Asia"],
    "\u6b27\u6d32": ["europe", "Europe"],
    "\u5317\u7f8e": ["north-america", "North America"],
    "\u5317\u7f8e\u6d32": ["north-america", "North America"],
    "\u5357\u7f8e": ["south-america", "South America"],
    "\u5927\u6d0b\u6d32": ["oceania", "Oceania"],
    "\u975e\u6d32": ["africa", "Africa"],
    "\u5176\u4ed6": ["other", "Other"]
};
def suffixes: [
    "\u8499\u53e4\u65cf\u85cf\u65cf\u81ea\u6cbb\u5dde", "\u50a3\u65cf\u666f\u9887\u65cf\u81ea\u6cbb\u5dde", "\u54c8\u5c3c\u65cf\u5f5d\u65cf\u81ea\u6cbb\u5dde", "\u58ee\u65cf\u82d7\u65cf\u81ea\u6cbb\u5dde", "\u82d7\u65cf\u4f97\u65cf\u81ea\u6cbb\u5dde", "\u5e03\u4f9d\u65cf\u82d7\u65cf\u81ea\u6cbb\u5dde", "\u571f\u5bb6\u65cf\u82d7\u65cf\u81ea\u6cbb\u5dde", "\u67ef\u5c14\u514b\u5b5c\u81ea\u6cbb\u5dde", "\u54c8\u8428\u514b\u81ea\u6cbb\u5dde", "\u5088\u50f3\u65cf\u81ea\u6cbb\u5dde", "\u8499\u53e4\u81ea\u6cbb\u5dde", "\u85cf\u65cf\u7f8c\u65cf\u81ea\u6cbb\u5dde", "\u85cf\u65cf\u81ea\u6cbb\u5dde", "\u767d\u65cf\u81ea\u6cbb\u5dde", "\u50a3\u65cf\u81ea\u6cbb\u5dde", "\u5f5d\u65cf\u81ea\u6cbb\u5dde", "\u56de\u65cf\u81ea\u6cbb\u5dde", "\u671d\u9c9c\u65cf\u81ea\u6cbb\u5dde", "\u81ea\u6cbb\u5dde", "\u5730\u533a", "\u65b0\u533a", "\u76df", "\u5e02"
];
def cities: {
        "\u5317\u4eac": "Beijing", "\u4e0a\u6d77": "Shanghai", "\u5929\u6d25": "Tianjin",
    "\u91cd\u5e86": "Chongqing", "\u77f3\u5bb6\u5e84": "Shijiazhuang", "\u5510\u5c71": "Tangshan",
    "\u79e6\u7687\u5c9b": "Qinhuangdao", "\u90af\u90f8": "Handan", "\u90a2\u53f0": "Xingtai",
    "\u4fdd\u5b9a": "Baoding", "\u5f20\u5bb6\u53e3": "Zhangjiakou", "\u627f\u5fb7": "Chengde",
    "\u6ca7\u5dde": "Cangzhou", "\u5eca\u574a": "Langfang", "\u8861\u6c34": "Hengshui",
    "\u96c4\u5b89": "Xiongan", "\u592a\u539f": "Taiyuan", "\u5927\u540c": "Datong",
    "\u9633\u6cc9": "Yangquan", "\u957f\u6cbb": "Changzhi", "\u664b\u57ce": "Jincheng",
    "\u6714\u5dde": "Shuozhou", "\u664b\u4e2d": "Jinzhong", "\u8fd0\u57ce": "Yuncheng",
    "\u5ffb\u5dde": "Xinzhou", "\u4e34\u6c7e": "Linfen", "\u5415\u6881": "Luliang",
    "\u547c\u548c\u6d69\u7279": "Hohhot", "\u5305\u5934": "Baotou", "\u4e4c\u6d77": "Wuhai",
    "\u8d64\u5cf0": "Chifeng", "\u901a\u8fbd": "Tongliao", "\u9102\u5c14\u591a\u65af": "Ordos",
    "\u547c\u4f26\u8d1d\u5c14": "Hulunbuir", "\u5df4\u5f66\u6dd6\u5c14": "Bayannur", "\u4e4c\u5170\u5bdf\u5e03": "Ulanqab",
    "\u5174\u5b89": "Hinggan", "\u9521\u6797\u90ed\u52d2": "Xilingol", "\u963f\u62c9\u5584": "Alxa",
    "\u6c88\u9633": "Shenyang", "\u5927\u8fde": "Dalian", "\u978d\u5c71": "Anshan",
    "\u629a\u987a": "Fushun", "\u672c\u6eaa": "Benxi", "\u4e39\u4e1c": "Dandong",
    "\u9526\u5dde": "Jinzhou", "\u8425\u53e3": "Yingkou", "\u961c\u65b0": "Fuxin",
    "\u8fbd\u9633": "Liaoyang", "\u76d8\u9526": "Panjin", "\u94c1\u5cad": "Tieling",
    "\u671d\u9633": "Chaoyang", "\u846b\u82a6\u5c9b": "Huludao", "\u957f\u6625": "Changchun",
    "\u5409\u6797": "Jilin", "\u56db\u5e73": "Siping", "\u8fbd\u6e90": "Liaoyuan",
    "\u901a\u5316": "Tonghua", "\u767d\u5c71": "Baishan", "\u677e\u539f": "Songyuan",
    "\u767d\u57ce": "Baicheng", "\u5ef6\u8fb9": "Yanbian", "\u54c8\u5c14\u6ee8": "Harbin",
    "\u9f50\u9f50\u54c8\u5c14": "Qiqihar", "\u9e21\u897f": "Jixi", "\u9e64\u5c97": "Hegang",
    "\u53cc\u9e2d\u5c71": "Shuangyashan", "\u5927\u5e86": "Daqing", "\u4f0a\u6625": "Yichun",
    "\u4f73\u6728\u65af": "Jiamusi", "\u4e03\u53f0\u6cb3": "Qitaihe", "\u7261\u4e39\u6c5f": "Mudanjiang",
    "\u9ed1\u6cb3": "Heihe", "\u7ee5\u5316": "Suihua", "\u5927\u5174\u5b89\u5cad": "Daxinganling",
    "\u5357\u4eac": "Nanjing", "\u65e0\u9521": "Wuxi", "\u5f90\u5dde": "Xuzhou",
    "\u5e38\u5dde": "Changzhou", "\u82cf\u5dde": "Suzhou", "\u5357\u901a": "Nantong",
    "\u8fde\u4e91\u6e2f": "Lianyungang", "\u6dee\u5b89": "Huaian", "\u76d0\u57ce": "Yancheng",
    "\u626c\u5dde": "Yangzhou", "\u9547\u6c5f": "Zhenjiang", "\u6cf0\u5dde": "Taizhou",
    "\u5bbf\u8fc1": "Suqian", "\u676d\u5dde": "Hangzhou", "\u5b81\u6ce2": "Ningbo",
    "\u6e29\u5dde": "Wenzhou", "\u5609\u5174": "Jiaxing", "\u6e56\u5dde": "Huzhou",
    "\u7ecd\u5174": "Shaoxing", "\u91d1\u534e": "Jinhua", "\u8862\u5dde": "Quzhou",
    "\u821f\u5c71": "Zhoushan", "\u53f0\u5dde": "Taizhou", "\u4e3d\u6c34": "Lishui",
    "\u5408\u80a5": "Hefei", "\u829c\u6e56": "Wuhu", "\u868c\u57e0": "Bengbu",
    "\u6dee\u5357": "Huainan", "\u9a6c\u978d\u5c71": "Maanshan", "\u6dee\u5317": "Huaibei",
    "\u94dc\u9675": "Tongling", "\u5b89\u5e86": "Anqing", "\u9ec4\u5c71": "Huangshan",
    "\u6ec1\u5dde": "Chuzhou", "\u961c\u9633": "Fuyang", "\u5bbf\u5dde": "Suzhou",
    "\u516d\u5b89": "Luan", "\u4eb3\u5dde": "Bozhou", "\u6c60\u5dde": "Chizhou",
    "\u5ba3\u57ce": "Xuancheng", "\u798f\u5dde": "Fuzhou", "\u53a6\u95e8": "Xiamen",
    "\u8386\u7530": "Putian", "\u4e09\u660e": "Sanming", "\u6cc9\u5dde": "Quanzhou",
    "\u6f33\u5dde": "Zhangzhou", "\u5357\u5e73": "Nanping", "\u9f99\u5ca9": "Longyan",
    "\u5b81\u5fb7": "Ningde", "\u5357\u660c": "Nanchang", "\u666f\u5fb7\u9547": "Jingdezhen",
    "\u840d\u4e61": "Pingxiang", "\u4e5d\u6c5f": "Jiujiang", "\u65b0\u4f59": "Xinyu",
    "\u9e70\u6f6d": "Yingtan", "\u8d63\u5dde": "Ganzhou", "\u5409\u5b89": "Jian",
    "\u5b9c\u6625": "Yichun", "\u629a\u5dde": "Fuzhou", "\u4e0a\u9976": "Shangrao",
    "\u6d4e\u5357": "Jinan", "\u9752\u5c9b": "Qingdao", "\u6dc4\u535a": "Zibo",
    "\u67a3\u5e84": "Zaozhuang", "\u4e1c\u8425": "Dongying", "\u70df\u53f0": "Yantai",
    "\u6f4d\u574a": "Weifang", "\u6d4e\u5b81": "Jining", "\u6cf0\u5b89": "Taian",
    "\u5a01\u6d77": "Weihai", "\u65e5\u7167": "Rizhao", "\u4e34\u6c82": "Linyi",
    "\u5fb7\u5dde": "Dezhou", "\u804a\u57ce": "Liaocheng", "\u6ee8\u5dde": "Binzhou",
    "\u83cf\u6cfd": "Heze", "\u90d1\u5dde": "Zhengzhou", "\u5f00\u5c01": "Kaifeng",
    "\u6d1b\u9633": "Luoyang", "\u5e73\u9876\u5c71": "Pingdingshan", "\u5b89\u9633": "Anyang",
    "\u9e64\u58c1": "Hebi", "\u65b0\u4e61": "Xinxiang", "\u7126\u4f5c": "Jiaozuo",
    "\u6fee\u9633": "Puyang", "\u8bb8\u660c": "Xuchang", "\u6f2f\u6cb3": "Luohe",
    "\u4e09\u95e8\u5ce1": "Sanmenxia", "\u5357\u9633": "Nanyang", "\u5546\u4e18": "Shangqiu",
    "\u4fe1\u9633": "Xinyang", "\u5468\u53e3": "Zhoukou", "\u9a7b\u9a6c\u5e97": "Zhumadian",
    "\u6d4e\u6e90": "Jiyuan", "\u6b66\u6c49": "Wuhan", "\u9ec4\u77f3": "Huangshi",
    "\u5341\u5830": "Shiyan", "\u5b9c\u660c": "Yichang", "\u8944\u9633": "Xiangyang",
    "\u9102\u5dde": "Ezhou", "\u8346\u95e8": "Jingmen", "\u5b5d\u611f": "Xiaogan",
    "\u8346\u5dde": "Jingzhou", "\u9ec4\u5188": "Huanggang", "\u54b8\u5b81": "Xianning",
    "\u968f\u5dde": "Suizhou", "\u6069\u65bd": "Enshi", "\u4ed9\u6843": "Xiantao",
    "\u6f5c\u6c5f": "Qianjiang", "\u5929\u95e8": "Tianmen", "\u795e\u519c\u67b6": "Shennongjia",
    "\u957f\u6c99": "Changsha", "\u682a\u6d32": "Zhuzhou", "\u6e58\u6f6d": "Xiangtan",
    "\u8861\u9633": "Hengyang", "\u90b5\u9633": "Shaoyang", "\u5cb3\u9633": "Yueyang",
    "\u5e38\u5fb7": "Changde", "\u5f20\u5bb6\u754c": "Zhangjiajie", "\u76ca\u9633": "Yiyang",
    "\u90f4\u5dde": "Chenzhou", "\u6c38\u5dde": "Yongzhou", "\u6000\u5316": "Huaihua",
    "\u5a04\u5e95": "Loudi", "\u6e58\u897f": "Xiangxi", "\u5e7f\u5dde": "Guangzhou",
    "\u97f6\u5173": "Shaoguan", "\u6df1\u5733": "Shenzhen", "\u73e0\u6d77": "Zhuhai",
    "\u6c55\u5934": "Shantou", "\u4f5b\u5c71": "Foshan", "\u6c5f\u95e8": "Jiangmen",
    "\u6e5b\u6c5f": "Zhanjiang", "\u8302\u540d": "Maoming", "\u8087\u5e86": "Zhaoqing",
    "\u60e0\u5dde": "Huizhou", "\u6885\u5dde": "Meizhou", "\u6c55\u5c3e": "Shanwei",
    "\u6cb3\u6e90": "Heyuan", "\u9633\u6c5f": "Yangjiang", "\u6e05\u8fdc": "Qingyuan",
    "\u4e1c\u839e": "Dongguan", "\u4e2d\u5c71": "Zhongshan", "\u6f6e\u5dde": "Chaozhou",
    "\u63ed\u9633": "Jieyang", "\u4e91\u6d6e": "Yunfu", "\u5357\u5b81": "Nanning",
    "\u67f3\u5dde": "Liuzhou", "\u6842\u6797": "Guilin", "\u68a7\u5dde": "Wuzhou",
    "\u5317\u6d77": "Beihai", "\u9632\u57ce\u6e2f": "Fangchenggang", "\u94a6\u5dde": "Qinzhou",
    "\u8d35\u6e2f": "Guigang", "\u7389\u6797": "Yulin", "\u767e\u8272": "Baise",
    "\u8d3a\u5dde": "Hezhou", "\u6cb3\u6c60": "Hechi", "\u6765\u5bbe": "Laibin",
    "\u5d07\u5de6": "Chongzuo", "\u6d77\u53e3": "Haikou", "\u4e09\u4e9a": "Sanya",
    "\u4e09\u6c99": "Sansha", "\u510b\u5dde": "Danzhou", "\u6210\u90fd": "Chengdu",
    "\u81ea\u8d21": "Zigong", "\u6500\u679d\u82b1": "Panzhihua", "\u6cf8\u5dde": "Luzhou",
    "\u5fb7\u9633": "Deyang", "\u7ef5\u9633": "Mianyang", "\u5e7f\u5143": "Guangyuan",
    "\u9042\u5b81": "Suining", "\u5185\u6c5f": "Neijiang", "\u4e50\u5c71": "Leshan",
    "\u5357\u5145": "Nanchong", "\u7709\u5c71": "Meishan", "\u5b9c\u5bbe": "Yibin",
    "\u5e7f\u5b89": "Guangan", "\u8fbe\u5dde": "Dazhou", "\u96c5\u5b89": "Yaan",
    "\u5df4\u4e2d": "Bazhong", "\u8d44\u9633": "Ziyang", "\u963f\u575d": "Aba",
    "\u7518\u5b5c": "Garze", "\u51c9\u5c71": "Liangshan", "\u8d35\u9633": "Guiyang",
    "\u516d\u76d8\u6c34": "Liupanshui", "\u9075\u4e49": "Zunyi", "\u5b89\u987a": "Anshun",
    "\u6bd5\u8282": "Bijie", "\u94dc\u4ec1": "Tongren", "\u9ed4\u897f\u5357": "Qianxinan",
    "\u9ed4\u4e1c\u5357": "Qiandongnan", "\u9ed4\u5357": "Qiannan", "\u6606\u660e": "Kunming",
    "\u66f2\u9756": "Qujing", "\u7389\u6eaa": "Yuxi", "\u4fdd\u5c71": "Baoshan",
    "\u662d\u901a": "Zhaotong", "\u4e3d\u6c5f": "Lijiang", "\u666e\u6d31": "Puer",
    "\u4e34\u6ca7": "Lincang", "\u695a\u96c4": "Chuxiong", "\u7ea2\u6cb3": "Honghe",
    "\u6587\u5c71": "Wenshan", "\u897f\u53cc\u7248\u7eb3": "Xishuangbanna", "\u5927\u7406": "Dali",
    "\u5fb7\u5b8f": "Dehong", "\u6012\u6c5f": "Nujiang", "\u8fea\u5e86": "Diqing",
    "\u62c9\u8428": "Lhasa", "\u65e5\u5580\u5219": "Shigatse", "\u660c\u90fd": "Chamdo",
    "\u6797\u829d": "Nyingchi", "\u5c71\u5357": "Shannan", "\u90a3\u66f2": "Nagqu",
    "\u963f\u91cc": "Ngari", "\u897f\u5b89": "Xian", "\u94dc\u5ddd": "Tongchuan",
    "\u5b9d\u9e21": "Baoji", "\u54b8\u9633": "Xianyang", "\u6e2d\u5357": "Weinan",
    "\u5ef6\u5b89": "Yanan", "\u6c49\u4e2d": "Hanzhong", "\u6986\u6797": "Yulin",
    "\u5b89\u5eb7": "Ankang", "\u5546\u6d1b": "Shangluo", "\u5170\u5dde": "Lanzhou",
    "\u5609\u5cea\u5173": "Jiayuguan", "\u91d1\u660c": "Jinchang", "\u767d\u94f6": "Baiyin",
    "\u5929\u6c34": "Tianshui", "\u6b66\u5a01": "Wuwei", "\u5f20\u6396": "Zhangye",
    "\u5e73\u51c9": "Pingliang", "\u9152\u6cc9": "Jiuquan", "\u5e86\u9633": "Qingyang",
    "\u5b9a\u897f": "Dingxi", "\u9647\u5357": "Longnan", "\u4e34\u590f": "Linxia",
    "\u7518\u5357": "Gannan", "\u897f\u5b81": "Xining", "\u6d77\u4e1c": "Haidong",
    "\u6d77\u5317": "Haibei", "\u9ec4\u5357": "Huangnan", "\u6d77\u5357": "Hainan",
    "\u679c\u6d1b": "Golog", "\u7389\u6811": "Yushu", "\u6d77\u897f": "Haixi",
    "\u683c\u5c14\u6728": "Golmud", "\u94f6\u5ddd": "Yinchuan", "\u77f3\u5634\u5c71": "Shizuishan",
    "\u5434\u5fe0": "Wuzhong", "\u56fa\u539f": "Guyuan", "\u4e2d\u536b": "Zhongwei",
    "\u4e4c\u9c81\u6728\u9f50": "Urumqi", "\u514b\u62c9\u739b\u4f9d": "Karamay", "\u5410\u9c81\u756a": "Turpan",
    "\u54c8\u5bc6": "Hami", "\u660c\u5409": "Changji", "\u535a\u5c14\u5854\u62c9": "Bortala",
    "\u5df4\u97f3\u90ed\u695e": "Bayingolin", "\u963f\u514b\u82cf": "Aksu", "\u514b\u5b5c\u52d2\u82cf": "Kizilsu",
    "\u5580\u4ec0": "Kashgar", "\u548c\u7530": "Hotan", "\u4f0a\u7281": "Ili",
    "\u5854\u57ce": "Tacheng", "\u963f\u52d2\u6cf0": "Altay", "\u77f3\u6cb3\u5b50": "Shihezi",
    "\u9999\u6e2f": "Hong Kong", "\u6fb3\u95e8": "Macau", "\u53f0\u5317": "Taipei",
    "\u9ad8\u96c4": "Kaohsiung", "\u65b0\u52a0\u5761": "Singapore", "\u4e1c\u4eac": "Tokyo",
    "\u5927\u962a": "Osaka", "\u9996\u5c14": "Seoul", "\u6cd5\u5170\u514b\u798f": "Frankfurt",
    "\u963f\u59c6\u65af\u7279\u4e39": "Amsterdam", "\u534e\u6c99": "Warsaw", "\u91cc\u65af\u672c": "Lisbon",
    "\u4f26\u6566": "London", "\u5df4\u9ece": "Paris", "\u6d1b\u6749\u77f6": "Los Angeles",
    "\u5723\u4f55\u585e": "San Jose", "\u7ebd\u7ea6": "New York", "\u6089\u5c3c": "Sydney"
};
def operators: {
    "\u7535\u4fe1": "telecom",
    "\u4e2d\u56fd\u7535\u4fe1": "telecom",
    "\u8054\u901a": "unicom",
    "\u4e2d\u56fd\u8054\u901a": "unicom",
    "\u79fb\u52a8": "mobile",
    "\u4e2d\u56fd\u79fb\u52a8": "mobile",
    "\u6559\u80b2\u7f51": "cernet",
    "\u4e2d\u56fd\u6559\u80b2\u7f51": "cernet",
    "\u5e7f\u7535\u7f51": "cbn",
    "\u5e7f\u7535": "cbn",
    "\u4e2d\u56fd\u5e7f\u7535": "cbn",
    "\u5176\u4ed6": "other"
};
def slugify:
    ascii_downcase | gsub("[^a-z0-9]+"; "-") | sub("^-+"; "") | sub("-+$"; "");
def strip_city_suffix:
    . as $raw
    | reduce suffixes[] as $suf ($raw;
        if (. | endswith($suf)) and (. | length) > ($suf | length) then
            .[:((. | length) - ($suf | length))]
        else . end);
def norm_operator($op; $name):
    if ($op != null and ($op | type == "string") and operators[$op] != null) then
        operators[$op]
    elif ($op | type == "string") and (["telecom","unicom","mobile","cernet","cbn","other"] | index($op | ascii_downcase)) != null then
        ($op | ascii_downcase)
    elif ($name | contains("\u7535\u4fe1")) then "telecom"
    elif ($name | contains("\u8054\u901a")) then "unicom"
    elif ($name | contains("\u79fb\u52a8")) then "mobile"
    elif ($name | contains("\u6559\u80b2")) then "cernet"
    elif ($name | contains("\u5e7f\u7535")) then "cbn"
    else "other" end;
def norm_province($p):
    if ($p | type == "string") and provinces[$p] != null then
        provinces[$p]
    elif ($p | type == "string") and ($p | whole("[ -~]{1,40}")) then
        [($p | slugify), $p]
    else
        ["other", "Other"]
    end;
def norm_city($c; $ip):
    ($c | if type == "string" then strip_city_suffix else "" end) as $stem
    | if cities[$stem] != null then
          cities[$stem]
      elif ($c | type == "string") and ($c | whole("[ -~]{1,40}")) then
          $c
      else
          "Node " + ($ip | gsub("\\."; "-"))
      end;
def op_rank:
    if . == "telecom" then 0
    elif . == "unicom" then 1
    elif . == "mobile" then 2
    elif . == "cernet" then 3
    elif . == "cbn" then 4
    else 5 end;
def loc_id($pslug; $city):
    ($city | slugify) as $cslug
    | if $pslug == $cslug then $pslug else "\($pslug)-\($cslug)" end
    | .[:40] | sub("-+$"; "");
def is_default_id:
    . == "beijing" or . == "shanghai" or . == "guangdong-guangzhou";
'

check_list() {
    [ -f "$1" ] || fail "no list at $1"
    size=$(wc -c < "$1" | tr -d ' ')
    [ "$size" -le "$LIMIT" ] || fail "$1 has $size bytes, more than $LIMIT"
    problems=$(jq -r "$RULES list_problems[]" "$1" 2>&1) || fail "$1 is not JSON: $problems"
    [ -z "$problems" ] || fail "$1 breaks the rules: $(printf '%s' "$problems" | tr '\n' ';')"
}

update() {
    [ -f "$1" ] || fail "no export at $1"
    jq -e 'type == "array" and all(.[]; type == "object" and (.ip | type == "string"))' "$1"         > /dev/null 2>&1 || fail "the export is not a JSON array of Taier nodes: not trusted"
    count=$(jq 'length' "$1")
    [ "$count" -ge 50 ] || fail "the export lists $count nodes, fewer than 50: not trusted"
    new=$(mktemp)
    trap 'rm -f "$new"' EXIT
    jq -r "$NORMALIZE"'
        [ .[]
          | (.ip // "" | tostring) as $ip
          | ((.port // "65499") | tostring | tonumber? // 0) as $port
          | select(($ip | ipv4) and $port >= 1024 and $port <= 65535)
          | norm_province(.p // "\u5176\u4ed6") as [$pslug, $pname]
          | norm_city(.c // ""; $ip) as $cname
          | loc_id($pslug; $cname) as $id
          | {
              id: $id,
              province: $pname,
              city: $cname,
              operator: norm_operator(.o // ""; .name // ""),
              ip: $ip,
              port: $port
            } ]
        | group_by(.id)
        | map(
            (.[0]) as $first
            | (map({operator, ip, port})
               | unique_by("\(.ip):\(.port)")
               | sort_by([(.operator | op_rank), .ip, .port])
               | .[:32]) as $nodes
            | ({
                id: $first.id,
                province: $first.province,
                city: $first.city
              }
              + (if ($first.id | is_default_id) then {default: true} else {} end)
              + {nodes: $nodes}))
        | sort_by(.id)
        | "{\"version\": 1, \"locations\": [", (map(tojson) | join(",\n")), "]}"' "$1" > "$new"
    check_list "$new"
    defaults=$(jq '[.locations[] | select(.default == true)] | length' "$new")
    [ "$defaults" -eq 3 ] ||
        fail "the new list has $defaults default locations, expected 3 (Beijing, Shanghai, Guangzhou)"
    if [ -f "$LIST" ]; then
        old=$(jq '.locations | length' "$LIST")
        lost=$(jq -n --slurpfile old "$LIST" --slurpfile new "$new" '
            [$old[0].locations[].id] - [$new[0].locations[].id] | length')
        [ $((lost * 3)) -le "$old" ] ||
            fail "the new list would lose $lost of the $old locations at once: not trusted"
        if cmp -s "$new" "$LIST"; then
            echo "unchanged"
            return
        fi
        jq -r -n --slurpfile old "$LIST" --slurpfile new "$new" '
            ($old[0].locations | map({key: .id, value: .}) | from_entries) as $before
            | ($new[0].locations | map({key: .id, value: .}) | from_entries) as $after
            | ([$before, $after] | add | keys_unsorted | unique)[]
            | . as $id
            | if $before[$id] == null then "\($id): \($after[$id].city), added"
              elif $after[$id] == null then "\($id): \($before[$id].city), removed"
              elif $before[$id] != $after[$id] then "\($id): \($after[$id].city), updated"
              else empty end'
    else
        echo "created"
    fi
    cp "$new" "$LIST"
}

case ${1:-} in
    check)
        check_list "$LIST"
        echo "taier-servers: $LIST follows the rules"
        ;;
    check-list)
        [ $# -eq 2 ] || fail "usage: scripts/taier-servers.sh check-list <file>"
        check_list "$2"
        ;;
    update)
        [ $# -eq 2 ] || fail "usage: scripts/taier-servers.sh update <export>"
        update "$2"
        ;;
    *)
        echo "usage: scripts/taier-servers.sh check | check-list <file> | update <export>" >&2
        exit 2
        ;;
esac
