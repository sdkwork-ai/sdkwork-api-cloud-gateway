# 鏁版嵁搴撳垵濮嬪寲鍏ㄩ噺鍥炲綊锛?026-11-02锛?
鑼冨洿锛歚D:\sdkwork-space` 涓嬪叏閮?`sdkwork-*` 鏁版嵁搴撴ā鍧椼€?鐩爣锛氶獙璇佹瘡涓ā鍧楀湪**鍏ㄦ柊 canonical PostgreSQL 搴?*涓婂畬鎴?init 鈫?migrate 鈫?seed锛?浜屾鎵ц骞傜瓑锛涙牳瀵?dev / test / staging / demo / production 浜斿鐜韬唤涓庡彂鐜拌緭鍏ワ紱
淇缂哄け鎴栭敊璇殑鍒濆鍖栨暟鎹€?
## 1. 鏂规硶涓庣煩闃?
| 缁村害 | 鎵ц鍐呭 |
| --- | --- |
| 妯″潡娓呭崟 | 71 涓０鏄?`database/database.manifest.json` 鐨勬ā鍧楋細69 涓?PostgreSQL锛? 涓?sqlite-only锛坅gentstudio銆乿ideo-cut锛屼笉鍙備笌 PG 鍥炲綊锛?|
| 鍥炲綊鏉＄洰 | 78 鏉?= 69 涓?root 妯″潡 + 9 涓０鏄庡瓙妯″潡锛坅ppbase `edu-data`/`med-data`锛宑loudrouter 7 涓瓙妯″潡锛?|
| 棣栨鍒濆鍖?| 鍦ㄥ叏鏂?`sdkwork_ai_prod` 涓婃寜 gateway `databaseBootstrapOrder` + 瀛楁瘝搴忛€愭潯 `sdkwork-db bootstrap` |
| 骞傜瓑 | 鍏ㄩ噺閲嶈窇绗簩閬嶏紝瑕佹眰 `0 migration(s), 0 seed script(s)` |
| 琛屾暟鏍￠獙 | 188 寮?seed 鐩爣琛ㄩ€愪竴 `count(*)`锛堢己澶辫〃璁?-1锛?|
| development | 瀵?20 涓?`seedOnBoot=true` 鐨?PG 妯″潡鍦ㄦ棦鏈?`sdkwork_ai_dev` 涓婃敹鏁涙墽琛?|
| staging / demo | 鏂板缓 canonical `sdkwork_ai_staging` / `sdkwork_ai_demo`锛岃窇 5 涓唬琛ㄦā鍧楋紙payment/appbase/iam/appstore/membership锛夛紝鏍￠獙鍚庡垹闄?|
| test | 閰嶇疆灞傛牎楠岋細`canonical_database_profile` 灏?`sdkwork_ai_test`銆乣sdkwork_ai_test_*` 鏄犲皠鍒?test profile锛涘叡浜祴璇曡鑹插嚟鎹綊娴嬭瘯宸ュ叿閾炬墍鏈夛紝鏈涓嶉噸缃?|

宸ュ叿锛歐SL Rust 鏋勫缓鐨?`sdkwork-db`锛坄sdkwork-database-cli`锛夛紝涓庡鍣?systemd 浣跨敤鐨勬槸鍚屼竴
`LifecycleOrchestrator`銆傚涓?Windows Rust 宸ュ叿閾剧己灏?`rustc`锛屾棤娉曞湪 Windows 渚ч噸缂?gateway锛?鍥犳 gateway 鐨勫樊寮備互浠ｇ爜璧版煡 + 鍗曞厓娴嬭瘯 + 鏁版嵁搴撶敓鍛藉懆鏈熷疄娴嬭鐩栥€?
## 2. 缁撴灉鎬昏

| 椤圭洰 | 缁撴灉 |
| --- | --- |
| 鐢熶骇鍨嬪叏鏂板簱棣栨鍒濆鍖?| 78 鏉′腑 76 鏉￠€氳繃锛? 涓ā鍧椾负宸茬煡闃诲锛堣 搂4锛?|
| 浜屾骞傜瓑 | 76 鏉￠€氳繃鏉＄洰鍏ㄩ儴 `0 migration(s), 0 seed script(s)` |
| dev 鏀舵暃 | 20 涓?`seedOnBoot=true` 妯″潡锛?8 閫氳繃锛? 涓负鐜鍘嗗彶闂锛堣 搂4锛?|
| staging / demo | 5/5 妯″潡鍏ㄩ儴閫氳繃锛屾暟鎹鏁颁笌 prod 涓€鑷?|
| 鍙戠幇鐨勭己闄?| 12 绫伙紝鍏ㄩ儴宸蹭慨澶嶅苟鎻愪氦锛?4 涓粨搴擄級 |

## 3. 宸蹭慨澶?
| 缂洪櫡 | 褰卞搷 | 淇 |
| --- | --- | --- |
| `seedOnBoot=false` 瑕嗙洊 14 涓壙杞芥潈濞佸垵濮嬪寲鏁版嵁鐨勬ā鍧?| 鍏ㄦ柊鐜 base 瀛楀吀銆佹敮浠樼洰褰曘€佺郴缁熼粯璁ゅ€笺€佹妧鑳姐€佷紒涓?闇€姹傘€佺ぞ鍖哄湀瀛愩€佸晢鍝佺洰褰曘€佽鍧涖€佹父鎴忕洰褰曘€侀偖浠舵ā鏉裤€丮CP 鍒嗙被銆佷紮浼寸瓑绾с€侀┍鍔ㄧ炕璇戙€侀粯璁ょ綉鍏虫嫇鎵戝叏閮ㄤ负绌?| 14 涓ā鍧?manifest 缃?`seedOnBoot=true`锛坈ommit 瑙?搂6锛?|
| 鍙戠幇杈撳叆閿悕涓嶅尮閰?| 瀹瑰櫒闀滃儚 8 涓?surface 鐨?`SDKWORK_<MODULE>_APP_ROOT` 涓?Rust 娲剧敓瑙勫垯涓嶄竴鑷达紙appbase/deployments/customerservice/gameengine 绛夛級锛岃繖浜?surface 鍦ㄥ鍣ㄤ腑浠庝笉琚彂鐜帮紱`pnpm dev` 瀹屽叏鏈０鏄庝换浣?surface锛屽彧鍒濆鍖?documents/models 鍥為€€鐩綍 | Dockerfile 琛ラ綈/淇 canonical 閿紱dev runner 鎵弿鍚岀骇妯″潡鑷姩瀵煎嚭锛堝惈鍗曟祴锛?|
| host-native 瀹夎鍣ㄤ笉鍐?surface 鏍?| 鐩磋鐢熶骇鏂瑰紡 discovery 閫変笉涓换浣?surface锛宍seedOnBoot` 妯″潡涓嶇亴鏁版嵁 | 瀹夎鍣ㄦ寜 `database-modules/*` 鐢熸垚 `SDKWORK_<MODULE>_APP_ROOT` |
| appstore `010_download_capabilities` 鐢?`listing_status='published'` | 005/017 褰掍竴鍖栦负 `active` 鍚庯紝鍏ㄦ柊搴?download grant / install event 涓?0 | 鎺ュ彈 `active` 涓?legacy `published` |
| appstore 005 鍐欏叆 `published` | 鍏ㄦ柊搴?78/235 涓笂鏋跺湪鐩綍鏌ヨ涓笉鍙 | 鍐欏叆 `active` 骞惰拷鍔?legacy 褰掍竴鍖?UPDATE |
| company demand hall seed 鍙岄噸缂栫爜鎹熷潖 | seed 瀹夊叏鏍￠獙鐩存帴鎷掔粷璇ユ枃浠讹紝闇€姹傚ぇ鍘呭湪鎵€鏈夌幆澧冧负绌?| 浠ュ悎娉?UTF-8 閲嶅啓 4 鏉￠渶姹?|
| canvas seed 澶嶅埗浜?settings 鐨?`cvs_system_setting` | seed 鎶?`relation does not exist`锛屾ā鍧楀垵濮嬪寲澶辫触 | 鏀逛负鏄惧紡 no-op锛坈anvas 鏃犲钩鍙拌缃〃锛?|
| compute seed manifest 缂?`profiles` | 浠讳綍 seed 闃舵鎶?`missing field profiles` | 琛?canonical 绌?profile / localeSets |
| missory legacy seed manifest 缁撴瀯 | locale 鏂囦欢寮曠敤 common 鑴氭湰銆乧hecksum 闈炴硶 | 杩佺Щ鍒版爣鍑?profiles + localeSets 缁撴瀯 |
| comments 鐨?AI 璇勬祴璇勮 seed 鏈寕 profile | 璇勬祴绾跨▼瀛樺湪浣嗚瘎璁鸿〃涓虹┖ | 鍔犲叆 standard profile |
| aiot/skills/webserver 閲嶅 `001_bootstrap.sql` | 鍚屼竴 seed 鎵ц涓ゆ | 鍘婚櫎閲嶅鏉＄洰 |
| localeSet checksum 涓庡唴瀹?琛屽熬涓嶄竴鑷?| CRLF 妫€鍑轰笂鐢熸垚鐨?checksum 鍦?LF 妫€鍑轰笂姘镐箙涓嶅尮閰嶏紱appbase/iam/membership/payment 绛夊０鏄庡€艰繃鏈?| 缁熶竴鎸?LF 瑙勮寖鍖栭噸绠楋紱appstore 鏍￠獙鑴氭湰鍚屾瑙勫垯 |

`seedOnBoot=true` 鏈€缁堥泦鍚堬紙20 涓?PG + 1 涓?sqlite锛夛細
`appbase`銆乣appstore`銆乣cloudrouter`銆乣community`銆乣company`銆乣course`銆乣documents`銆?`drive`銆乣forum`銆乣gameengine`銆乣iam`銆乣mail`銆乣mcp`銆乣membership`銆乣merchandise`銆?`order`銆乣partner`銆乣payment`銆乣settings`銆乣skills`锛泂qlite 渚?`agentstudio`銆?
淇濇寔 opt-in锛坄seedOnBoot=false`锛夌殑绀轰緥/杩愯惀鏁版嵁妯″潡锛?`im`锛坉emo 鐢ㄦ埛鏁版嵁锛夈€乣comments`锛堣瘎娴?demo 璇勮锛夈€乣github`锛坉emo锛夈€乣dezhou`
锛坉emo tenant锛夈€乣shop`锛堟敞閲婃帀鐨勭ず渚嬶級銆乣aiot`锛坉ev/pilot 妯℃澘锛夈€?
## 4. 鏈慨澶?/ 闇€浜哄伐鍐崇瓥

1. **`sdkwork-games` 涓?`sdkwork-gameengine` 閲嶅 moduleId `games`**
   涓よ€呴兘澹版槑 `moduleId=games`銆乣tablePrefix=game_` 骞跺啓鍚屼竴鎵?`game_*` 琛紱鍚屾椂
   bootstrap 浼氳Е鍙?`game_catalog_uuid_key` 鍞竴閿啿绐併€俙sdkwork-games` 鐨?README 澹版槑
   鏍稿績鑳藉姏鐢?`sdkwork-gameengine` 灏佽銆佷笉搴旈噸澶嶏紝gateway 涔熸湭鎶婂畠浣滀负 surface銆?   寤鸿锛氱Щ闄?`sdkwork-games/database/`锛堝睘鐮村潖鎬у彉鏇达紝闇€浜哄伐纭锛夋垨涓哄叾鍒嗛厤鐙珛
   moduleId 涓庤〃鍓嶇紑銆?*褰撳墠鍥炲綊鐭╅樀涓畠鏄敮涓€纭畾鎬уけ璐ラ」銆?*
2. **course 0001 杩佺Щ琚氨鍦颁慨鏀?*
   `981dd10` 鍦?0001 涓垹闄や簡 5 琛?SQL锛?026-10-30锛夛紝鑰?dev 宸叉寜鏃?checksum 搴旂敤锛?   鐜板湪 `checksum_mismatch` fail-closed銆備换浣曞凡搴旂敤鏃?0001 鐨勭幆澧冧笅娆?bootstrap 閮戒細澶辫触銆?   寤鸿锛氭仮澶?0001 鍘熷鍐呭骞舵妸鍙樻洿鏀捐繘鏂拌縼绉伙紝鎴栬蛋浜哄伐璇勫鐨?history 鏍℃娴佺▼銆?3. **company 浠呮湁 baseline銆佹病鏈夎縼绉?*
   `company_demand_category` / `company_demand_bid` 鍙湪 baseline 涓紱dev 鐨勬棫 schema
   涓嶄細琚?baseline 鏇存柊锛宻eed 鎶?`relation does not exist`銆傞渶琛?additive migration
   `0001_company_demand_hall`锛坰chema 鍙樻洿锛岄渶浜哄伐纭锛夈€?4. **im demo seed 渚濊禆 IAM demo 鐢ㄦ埛**
   `002_im_demo_data` 鏄庣‘瑕佹眰 owner/guest/alice/bob/grace锛沗seedOnBoot=false` 涓嬬敓浜?   涓嶅彈褰卞搷锛屾樉寮?seed 鍓嶉渶杩愯 `scripts/dev/ensure-im-h5-demo-users.mjs`銆?5. **payment 杩佺Щ鍏冩暟鎹笉鍚堣**锛坄check-database-framework-standard` 鍞竴澶辫触椤癸級
   0002/0003 缂?reversible/rollback/transactional 绛夊ご閮ㄥ瓧娈碉紝灞炶縼绉绘不鐞嗛棶棰樸€?6. **knowledgebase baseline 鍐呭惈 `CREATE EXTENSION IF NOT EXISTS vector`**
   app 瑙掕壊鏃犳潈闄愶紱姝ｅ父璺緞鐢?`--provision-databases` 浠ョ鐞嗗憳棰勫缓銆傝嫢杩愮淮璺宠繃
   provision锛屾ā鍧楀垵濮嬪寲浼氬け璐ャ€?7. **appstore `015_vertical_evaluation.sql` 浠嶄负瀛ゅ効 seed**
   瀹冩病鏈夊搴?zh/en locale 鏂囨锛涚洿鎺ュ惎鐢ㄤ細璁?9 涓簲鐢ㄥ洖閫€鏄剧ず app_key銆傞渶鍏堣ˉ
   localization 鍐嶇撼鍏?profile銆?8. **en-US 鏂囨鏈殢鍚姩 seed**
   鍚姩鎸?`SDKWORK_DATABASE_SEED_LOCALE=zh-CN` 鎵ц锛涢渶瑕佽嫳鏂囨枃妗堢殑鐜搴旀樉寮?   `pnpm db:seed --locale en-US`锛坉ev 宸插弻璇枃妗堬級銆?
## 5. 鍏抽敭璇佹嵁

- 鍏ㄦ柊 `sdkwork_ai_prod`锛歚appstore` 235 鏉?`active/visible` 涓婃灦銆?4 download grant銆?  84 install event銆?35 鍚堣妗ｆ銆?70 鍖哄煙鍙敤鎬э紱`base_country` 252锛沗company_demand`
  6锛沗comments_comment` 126锛沗membership_plan` 6锛沗commerce_payment_method` 17銆?- `ops_database_installation_state`锛歛ppstore / base-data / payment / settings / skills /
  company / community / merchandise / forum / mail / mcp / partner / drive / cloudrouter /
  iam / membership / order / course / documents 鍧囦负 `seeded`銆?- staging/demo 鍐掔儫锛? 妯″潡鍧?`bootstrap complete`锛岃鏁颁笌 prod 涓€鑷淬€?- dev 鏀舵暃锛?8/20 妯″潡 `0 pending` 鎴栦粎搴旂敤鍙樻洿 seed锛涘け璐ラ」涓?搂4.2/搂4.3 鐨勫巻鍙?schema 闂銆?
## 6. 鎻愪氦

| 浠撳簱 | commit | 鍐呭 |
| --- | --- | --- |
| sdkwork-api-cloud-gateway | `bbeeee1` | Dockerfile surface 閿€乨ev runner 鍙戠幇銆佽繍缁存枃妗?|
| sdkwork-appstore | `ae828ae`锛堝強 `745e7bd`锛?| download grant 鐘舵€併€乻eedOnBoot銆?05/001/015銆乧hecksum 瑙勫垯 |
| sdkwork-appbase | `b577bf9` | seedOnBoot + checksum |
| sdkwork-payment | `4b16f9f` | seedOnBoot + checksum |
| sdkwork-settings | `7f09045` | seedOnBoot |
| sdkwork-skills | `d1757be` | seedOnBoot + 鍘婚噸 + checksum |
| sdkwork-company | `8ae0e9f` | seedOnBoot + 闇€姹?seed 淇 |
| sdkwork-community | `ebfe7f6` | seedOnBoot |
| sdkwork-merchandise | `07f6d26` | seedOnBoot |
| sdkwork-forum | `b7102d8` | seedOnBoot |
| sdkwork-gameengine | `9f98d3e` | seedOnBoot |
| sdkwork-mail | `f5a14b9` | seedOnBoot |
| sdkwork-mcp | `ea32557` | seedOnBoot |
| sdkwork-partner | `79d377b` | seedOnBoot + checksum |
| sdkwork-drive | `b3acbf7` | seedOnBoot + checksum |
| sdkwork-cloudrouter | `9c1952e` | seedOnBoot + checksum |
| sdkwork-iam | `b1ce1cd` | checksum |
| sdkwork-membership | `03dc762` | checksum |
| sdkwork-canvas | `0bc9c65` | seed no-op |
| sdkwork-compute | `3837a2b` | manifest |
| sdkwork-missory | `2640ecf` | manifest |
| sdkwork-comments | `2fbd963` | profile 鎸傝浇 |
| sdkwork-aiot | `bf6ef0e` | 鍘婚噸 |
| sdkwork-webserver | `5f55ab3` | 鍘婚噸 |

## 7. 澶嶇幇

```bash
# 鍏ㄩ噺鍥炲綊锛圵SL锛宻dkwork-db 宸叉瀯寤猴級
bash /mnt/d/sdkwork-space/_scratch/db-init-regression/run-regression.sh
# dev 鏀舵暃
bash /mnt/d/sdkwork-space/_scratch/db-init-regression/dev-convergence.sh
# staging/demo 鍐掔儫
bash /mnt/d/sdkwork-space/_scratch/db-init-regression/env-smoke.sh
```

鑴氭湰涓庡師濮嬩骇鐗╀綅浜?`D:\sdkwork-space\_scratch\db-init-regression\`锛坕nventory / entries /
runs / module-ledger / table-counts / failures锛夈€?
