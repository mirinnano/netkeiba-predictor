use crate::models::{
    combo_key, ComboOdds, HorseRun, HorseEntry, KaisaiCalendarResponse, KaisaiDateItem, OfficialPayoutItem,
    PastRun, RaceInfo, RaceSummary, ResultRow, VenueRaces,
};
use encoding_rs::EUC_JP;
use regex::Regex;
use scraper::{ElementRef, Html, Selector};
use serde_json::Value;
use std::collections::HashMap;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

pub fn build_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| format!("HTTPクライアント初期化エラー: {}", e))
}

pub async fn fetch_html(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let resp = client
        .get(url)
        .header("Accept-Language", "ja,en-US;q=0.9,en;q=0.8")
        .send()
        .await
        .map_err(|e| format!("通信エラー ({}): {}", url, e))?;

    if !resp.status().is_success() {
        return Err(format!(
            "サーバーがステータス {} を返しました ({})",
            resp.status(),
            url
        ));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("レスポンス読込エラー: {}", e))?;

    // Check if strict UTF-8 first; if not, fallback to EUC-JP
    if let Ok(utf8_str) = std::str::from_utf8(&bytes) {
        Ok(utf8_str.to_string())
    } else {
        let (cow, _, _) = EUC_JP.decode(&bytes);
        Ok(cow.into_owned())
    }
}

pub fn extract_race_id(input: &str) -> Result<(String, bool), String> {
    let re = Regex::new(r"(\d{12})").unwrap();
    if let Some(caps) = re.captures(input.trim()) {
        let race_id = caps[1].to_string();
        let venue_code: u32 = race_id[4..6].parse().unwrap_or(5);
        let is_nar = input.contains("nar.netkeiba.com") || venue_code >= 30;
        Ok((race_id, is_nar))
    } else {
        Err("有効な12桁のレースID（例: 202305050812）またはnetkeibaの出馬表URLを入力してください。".to_string())
    }
}

pub fn venue_code_to_name(code: &str) -> &'static str {
    match code {
        "01" => "札幌",
        "02" => "函館",
        "03" => "福島",
        "04" => "新潟",
        "05" => "東京",
        "06" => "中山",
        "07" => "中京",
        "08" => "京都",
        "09" => "阪神",
        "10" => "小倉",
        "30" => "門別",
        "35" => "盛岡",
        "36" => "水沢",
        "42" => "浦和",
        "43" => "船橋",
        "44" => "大井",
        "45" => "川崎",
        "46" => "金沢",
        "47" => "笠松",
        "48" => "名古屋",
        "50" => "園田",
        "51" => "姫路",
        "54" => "高知",
        "55" => "佐賀",
        _ => "競馬場",
    }
}

fn parse_grade_from_classes(html_fragment: &str) -> Option<String> {
    if html_fragment.contains("Icon_GradeType1 ")
        || html_fragment.contains("Icon_GradeType1\"")
    {
        Some("GI".to_string())
    } else if html_fragment.contains("Icon_GradeType2 ")
        || html_fragment.contains("Icon_GradeType2\"")
    {
        Some("GII".to_string())
    } else if html_fragment.contains("Icon_GradeType3 ")
        || html_fragment.contains("Icon_GradeType3\"")
    {
        Some("GIII".to_string())
    } else if html_fragment.contains("Icon_GradeType10") {
        Some("J・GI".to_string())
    } else if html_fragment.contains("Icon_GradeType11") {
        Some("J・GII".to_string())
    } else if html_fragment.contains("Icon_GradeType12") {
        Some("J・GIII".to_string())
    } else if html_fragment.contains("Icon_GradeType15") {
        Some("L".to_string())
    } else if html_fragment.contains("Icon_GradeType5") {
        Some("OP".to_string())
    } else {
        None
    }
}

fn parse_weather_from_classes(html_fragment: &str) -> String {
    if html_fragment.contains("Weather01") {
        "晴".to_string()
    } else if html_fragment.contains("Weather02") {
        "曇".to_string()
    } else if html_fragment.contains("Weather03") {
        "雨".to_string()
    } else if html_fragment.contains("Weather04") {
        "小雨".to_string()
    } else if html_fragment.contains("Weather05") {
        "雪".to_string()
    } else if html_fragment.contains("Weather06") {
        "小雪".to_string()
    } else {
        "晴".to_string()
    }
}

/// コース文字列から馬場種別を判定する。
/// 障害戦は "障4100m 芝" や "障3170(芝 内-外)" のように「芝」も含むので、「障」を先に見る。
pub(crate) fn detect_surface(txt: &str) -> String {
    if txt.contains('障') {
        "障".to_string()
    } else if txt.contains('芝') {
        "芝".to_string()
    } else if txt.contains('ダ') {
        "ダ".to_string()
    } else {
        "芝".to_string()
    }
}

/// レース名やグレードから障害戦かを判定する。
/// 枠順確定前の障害戦は netkeiba でも "芝2750m" としか表示されないため、コース文字列だけでは判定できない。
pub(crate) fn is_jump_race(race_name: &str, grade: Option<&str>) -> bool {
    race_name.contains("障害")
        || race_name.contains("ジャンプ")
        || grade.map(|g| g.starts_with('J')).unwrap_or(false)
}

/// 過去走のレース名。<a> 直下のテキストだけを使い、グレードアイコンの <span> は除く。
pub(crate) fn race_name_from_link(el: ElementRef) -> String {
    let direct: String = el
        .children()
        .filter_map(|c| c.value().as_text().map(|t| t.to_string()))
        .collect();
    let name = clean_text(&direct);
    if name.is_empty() {
        clean_text(&el.text().collect::<String>())
    } else {
        name
    }
}

fn clean_text(s: &str) -> String {
    s.replace('\u{a0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub async fn fetch_kaisai_dates(
    target_date: Option<String>,
    is_nar: bool,
) -> Result<KaisaiCalendarResponse, String> {
    let client = build_client()?;
    let base = if is_nar {
        "https://nar.netkeiba.com"
    } else {
        "https://race.netkeiba.com"
    };
    let url = match &target_date {
        Some(d) if !d.trim().is_empty() => format!(
            "{}/top/race_list_get_date_list.html?kaisai_date={}",
            base,
            d.trim()
        ),
        _ => format!("{}/top/race_list_get_date_list.html", base),
    };

    let html = fetch_html(&client, &url).await?;
    let doc = Html::parse_fragment(&html);

    let prev_sel = Selector::parse("#prevBtn").unwrap();
    let next_sel = Selector::parse("#nextBtn").unwrap();
    let li_sel = Selector::parse("#date_list_sub li").unwrap();
    let a_sel = Selector::parse("a").unwrap();

    let prev_date = doc
        .select(&prev_sel)
        .next()
        .and_then(|el| el.value().attr("date"))
        .map(|s| s.to_string());

    let next_date = doc
        .select(&next_sel)
        .next()
        .and_then(|el| el.value().attr("date"))
        .map(|s| s.to_string());

    let mut dates = Vec::new();
    let mut active_date_from_dom: Option<String> = None;

    for li in doc.select(&li_sel) {
        if let Some(date_attr) = li.value().attr("date") {
            if li
                .value()
                .attr("class")
                .map(|c| c.contains("Active"))
                .unwrap_or(false)
            {
                active_date_from_dom = Some(date_attr.to_string());
            }
            let label = li
                .select(&a_sel)
                .next()
                .map(|a| {
                    a.value()
                        .attr("title")
                        .map(|t| t.to_string())
                        .unwrap_or_else(|| clean_text(&a.text().collect::<String>()))
                })
                .unwrap_or_else(|| date_attr.to_string());
            let inner = li.inner_html();
            let is_saturday = inner.contains("Sat") || label.contains("(土)");
            let is_sunday = inner.contains("Sun") || label.contains("(日)");
            dates.push(KaisaiDateItem {
                date: date_attr.to_string(),
                label,
                is_saturday,
                is_sunday,
            });
        }
    }

    let current_target = target_date
        .filter(|s| !s.is_empty())
        .or(active_date_from_dom)
        .or_else(|| {
            if !is_nar && dates.len() >= 3 {
                // Prefer the current weekend Saturday/Sunday rather than 2 weeks ahead
                Some(dates[dates.len() - 2].date.clone())
            } else {
                dates.last().map(|d| d.date.clone())
            }
        })
        .unwrap_or_else(|| {
            if is_nar {
                "20261112".to_string()
            } else {
                "20261108".to_string()
            }
        });

    Ok(KaisaiCalendarResponse {
        current_target,
        prev_date,
        next_date,
        dates,
    })
}

pub async fn fetch_race_list(kaisai_date: &str, is_nar: bool) -> Result<Vec<VenueRaces>, String> {
    let client = build_client()?;
    let base = if is_nar {
        "https://nar.netkeiba.com"
    } else {
        "https://race.netkeiba.com"
    };
    let url = format!(
        "{}/top/race_list_sub.html?kaisai_date={}",
        base,
        kaisai_date.trim()
    );
    let html = fetch_html(&client, &url).await?;
    let doc = Html::parse_fragment(&html);

    let dl_sel = Selector::parse("dl.RaceList_DataList").unwrap();
    let title_sel = Selector::parse("p.RaceList_DataTitle").unwrap();
    let shiba_sel = Selector::parse("span.Shiba").unwrap();
    let da_sel = Selector::parse("span.Da").unwrap();
    let li_sel = Selector::parse("li.RaceList_DataItem").unwrap();
    let a_sel = Selector::parse("a").unwrap();
    let num_sel = Selector::parse("div.Race_Num").unwrap();
    let item_title_sel = Selector::parse("span.ItemTitle").unwrap();
    let time_sel = Selector::parse("span.RaceList_Itemtime").unwrap();
    let long_sel = Selector::parse("span.RaceList_ItemLong").unwrap();
    let count_sel = Selector::parse("span.RaceList_Itemnumber").unwrap();

    let re_race_id = Regex::new(r"race_id=(\d{12})").unwrap();
    let re_dist = Regex::new(r"(\d{3,4})").unwrap();
    let re_count = Regex::new(r"(\d+)").unwrap();

    let mut venues = Vec::new();

    for dl in doc.select(&dl_sel) {
        let venue_name = dl
            .select(&title_sel)
            .next()
            .map(|el| clean_text(&el.text().collect::<String>()))
            .unwrap_or_else(|| "開催競馬場".to_string());

        let short_venue = [
            "札幌", "函館", "福島", "新潟", "東京", "中山", "中京", "京都", "阪神", "小倉",
            "門別", "帯広", "盛岡", "水沢", "浦和", "船橋", "大井", "川崎", "金沢", "笠松",
            "名古屋", "園田", "姫路", "高知", "佐賀",
        ]
        .iter()
        .find(|&&v| venue_name.contains(v))
        .unwrap_or(if is_nar { &"地方" } else { &"中央" })
        .to_string();

        let weather = parse_weather_from_classes(&dl.inner_html());
        let turf_cond = dl
            .select(&shiba_sel)
            .next()
            .map(|el| clean_text(&el.text().collect::<String>()))
            .unwrap_or_default();
        let dirt_cond = dl
            .select(&da_sel)
            .next()
            .map(|el| clean_text(&el.text().collect::<String>()))
            .unwrap_or_default();

        let mut races = Vec::new();
        for li in dl.select(&li_sel) {
            let mut race_id = None;
            for a in li.select(&a_sel) {
                if let Some(href) = a.value().attr("href") {
                    if let Some(caps) = re_race_id.captures(href) {
                        race_id = Some(caps[1].to_string());
                        break;
                    }
                }
            }
            let Some(race_id) = race_id else { continue };

            let race_num = li
                .select(&num_sel)
                .next()
                .map(|el| clean_text(&el.text().collect::<String>()))
                .unwrap_or_else(|| format!("{}R", &race_id[10..12]));

            let title = li
                .select(&item_title_sel)
                .next()
                .map(|el| clean_text(&el.text().collect::<String>()))
                .unwrap_or_else(|| "レース".to_string());

            let grade = parse_grade_from_classes(&li.inner_html());

            let start_time = li
                .select(&time_sel)
                .next()
                .map(|el| clean_text(&el.text().collect::<String>()))
                .unwrap_or_default();

            let long_txt = li
                .select(&long_sel)
                .next()
                .map(|el| clean_text(&el.text().collect::<String>()))
                .unwrap_or_default();

            let surface = if is_jump_race(&title, grade.as_deref()) {
                "障".to_string()
            } else {
                detect_surface(&long_txt)
            };

            let distance = re_dist
                .captures(&long_txt)
                .and_then(|c| c[1].parse::<u32>().ok())
                .unwrap_or(1600);

            let count_txt = li
                .select(&count_sel)
                .next()
                .map(|el| clean_text(&el.text().collect::<String>()))
                .unwrap_or_default();
            let head_count = re_count
                .captures(&count_txt)
                .and_then(|c| c[1].parse::<u32>().ok())
                .unwrap_or(0);

            races.push(RaceSummary {
                race_id,
                race_num,
                title,
                grade,
                start_time,
                surface,
                distance,
                head_count,
            });
        }

        if !races.is_empty() {
            venues.push(VenueRaces {
                venue_name,
                short_venue,
                weather,
                turf_cond,
                dirt_cond,
                races,
            });
        }
    }

    Ok(venues)
}

fn parse_time_to_seconds(time_str: &str) -> Option<f64> {
    let re = Regex::new(r"(?:(\d+):)?(\d{1,2})\.(\d)").unwrap();
    let caps = re.captures(time_str)?;
    let mins: f64 = caps
        .get(1)
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0.0);
    let secs: f64 = caps.get(2)?.as_str().parse().ok()?;
    let tenths: f64 = caps.get(3)?.as_str().parse().ok()?;
    Some(mins * 60.0 + secs + tenths * 0.1)
}

fn parse_past_run(td: ElementRef) -> Option<PastRun> {
    let item_sel = Selector::parse("div.Data_Item").unwrap();
    let item = td.select(&item_sel).next()?;

    let d01_sel = Selector::parse("div.Data01 span").unwrap();
    let num_sel = Selector::parse("div.Data01 span.Num").unwrap();
    let d02_sel = Selector::parse("div.Data02 a").unwrap();
    let d05_sel = Selector::parse("div.Data05").unwrap();
    let d05_strong_sel = Selector::parse("div.Data05 strong").unwrap();
    let d03_sel = Selector::parse("div.Data03").unwrap();
    let d06_sel = Selector::parse("div.Data06").unwrap();
    let d07_sel = Selector::parse("div.Data07").unwrap();

    let d01_first = item
        .select(&d01_sel)
        .next()
        .map(|el| clean_text(&el.text().collect::<String>()))
        .unwrap_or_default();
    let mut parts01 = d01_first.split_whitespace();
    let date = parts01.next().unwrap_or("").to_string();
    let venue = parts01.next().unwrap_or("").to_string();

    let finish_rank = item
        .select(&num_sel)
        .next()
        .and_then(|el| clean_text(&el.text().collect::<String>()).parse::<u32>().ok());

    let d02_el = item.select(&d02_sel).next();
    let race_name = d02_el.map(race_name_from_link).unwrap_or_default();
    let grade = d02_el.and_then(|el| parse_grade_from_classes(&el.inner_html()));

    let d05_txt = item
        .select(&d05_sel)
        .next()
        .map(|el| clean_text(&el.text().collect::<String>()))
        .unwrap_or_default();
    let track_cond = item
        .select(&d05_strong_sel)
        .next()
        .map(|el| clean_text(&el.text().collect::<String>()))
        .unwrap_or_else(|| "良".to_string());

    let surface = detect_surface(&d05_txt);

    let re_dist = Regex::new(r"(\d{3,4})").unwrap();
    let distance = re_dist
        .captures(&d05_txt)
        .and_then(|c| c[1].parse::<u32>().ok())
        .unwrap_or(1600);

    let re_time = Regex::new(r"(\d+:\d{2}\.\d|\d{2}\.\d)").unwrap();
    let time_str = re_time
        .captures(&d05_txt)
        .map(|c| c[1].to_string())
        .unwrap_or_default();
    let time_sec = parse_time_to_seconds(&time_str);

    // Data03: "18頭 6番 1人 川田将雅 55.0"
    let d03_txt = item
        .select(&d03_sel)
        .next()
        .map(|el| clean_text(&el.text().collect::<String>()))
        .unwrap_or_default();

    let re_head = Regex::new(r"(\d+)頭").unwrap();
    let re_umaban = Regex::new(r"(\d+)番").unwrap();
    let re_pop = Regex::new(r"(\d+)人").unwrap();
    let re_weight = Regex::new(r"(\d{2}\.\d)").unwrap();

    let head_count = re_head
        .captures(&d03_txt)
        .and_then(|c| c[1].parse::<u32>().ok());
    let horse_num = re_umaban
        .captures(&d03_txt)
        .and_then(|c| c[1].parse::<u32>().ok());
    let popularity = re_pop
        .captures(&d03_txt)
        .and_then(|c| c[1].parse::<u32>().ok());
    let carried_weight = re_weight
        .captures(&d03_txt)
        .and_then(|c| c[1].parse::<f64>().ok());

    let mut jockey = String::new();
    for token in d03_txt.split_whitespace() {
        if !token.ends_with('頭')
            && !token.ends_with('番')
            && !token.ends_with('人')
            && token.parse::<f64>().is_err()
        {
            jockey = token.to_string();
            break;
        }
    }

    // Data06: "6-7-7-3 (33.6) 476(+10)"
    let d06_txt = item
        .select(&d06_sel)
        .next()
        .map(|el| clean_text(&el.text().collect::<String>()))
        .unwrap_or_default();
    let passage = d06_txt
        .split_whitespace()
        .next()
        .filter(|s| s.contains('-'))
        .unwrap_or("")
        .to_string();
    let re_3f = Regex::new(r"\((\d{2}\.\d)\)").unwrap();
    let last_3f = re_3f
        .captures(&d06_txt)
        .and_then(|c| c[1].parse::<f64>().ok());

    // Data07: "マスクトディーヴァ(-0.1)"
    let d07_txt = item
        .select(&d07_sel)
        .next()
        .map(|el| clean_text(&el.text().collect::<String>()))
        .unwrap_or_default();
    let re_margin = Regex::new(r"\(([+-]?\d+\.\d)\)").unwrap();
    let margin = re_margin
        .captures(&d07_txt)
        .and_then(|c| c[1].parse::<f64>().ok());

    Some(PastRun {
        date,
        venue,
        race_name,
        grade,
        finish_rank,
        surface,
        distance,
        time_str,
        time_sec,
        track_cond,
        head_count,
        horse_num,
        popularity,
        jockey,
        carried_weight,
        passage,
        last_3f,
        margin,
    })
}

pub async fn fetch_jra_odds(
    client: &reqwest::Client,
    race_id: &str,
) -> HashMap<u32, (Option<f64>, Option<f64>, Option<f64>, Option<u32>)> {
    let mut map = HashMap::new();
    let urls = [
        format!(
            "https://race.netkeiba.com/api/api_get_jra_odds.html?pid=api_get_jra_odds&input=UTF-8&output=json&race_id={}&type=1&action=init&compress=0",
            race_id
        ),
        format!(
            "https://race.netkeiba.com/api/api_get_jra_odds.html?pid=api_get_jra_odds&input=UTF-8&output=json&race_id={}&type=1&compress=0",
            race_id
        ),
    ];

    for url in &urls {
        let Ok(text) = fetch_html(client, url).await else {
            continue;
        };
        let Ok(json) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let Some(odds_obj) = json
            .get("data")
            .and_then(|d| d.get("odds"))
            .and_then(|o| o.as_object())
        else {
            continue;
        };

        let win_map = odds_obj.get("1").and_then(|v| v.as_object());
        let place_map = odds_obj.get("2").and_then(|v| v.as_object());

        if let Some(w_obj) = win_map {
            for (k, arr_val) in w_obj {
                let Ok(umaban) = k.parse::<u32>() else {
                    continue;
                };
                if let Some(arr) = arr_val.as_array() {
                    let win_odds = arr
                        .first()
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.replace(',', "").parse::<f64>().ok())
                        .filter(|&v| v > 0.0);
                    let pop = arr
                        .get(2)
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<u32>().ok())
                        .filter(|&p| p > 0);
                    let entry = map.entry(umaban).or_insert((None, None, None, None));
                    entry.0 = win_odds;
                    entry.3 = pop;
                }
            }
        }

        if let Some(p_obj) = place_map {
            for (k, arr_val) in p_obj {
                let Ok(umaban) = k.parse::<u32>() else {
                    continue;
                };
                if let Some(arr) = arr_val.as_array() {
                    let p_min = arr
                        .first()
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.replace(',', "").parse::<f64>().ok())
                        .filter(|&v| v > 0.0);
                    let p_max = arr
                        .get(1)
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.replace(',', "").parse::<f64>().ok())
                        .filter(|&v| v > 0.0);
                    let entry = map.entry(umaban).or_insert((None, None, None, None));
                    entry.1 = p_min;
                    entry.2 = p_max;
                }
            }
        }

        if !map.is_empty() {
            break;
        }
    }

    map
}

/// result.html の着順表を読む。着順が数字の行（＝確定した行）だけを返す。
fn parse_result_rows(doc: &Html) -> Vec<ResultRow> {
    let tr_sel = Selector::parse("#All_Result_Table tr.HorseList, table.RaceTable01 tr.HorseList").unwrap();
    let rank_sel = Selector::parse("td.Result_Num div.Rank, td.Result_Num").unwrap();
    let umaban_sel = Selector::parse("td.Num.Txt_C div, td.Num.Txt_C").unwrap();
    let time_sel = Selector::parse("td.Time span.RaceTime").unwrap();
    let pop_sel = Selector::parse("td.Odds span.OddsPeople").unwrap();
    let odds_sel = Selector::parse("td.Odds span.Odds_Ninki").unwrap();
    let bg_time_sel = Selector::parse("td.Time").unwrap();
    let passage_sel = Selector::parse("td.PassageRate").unwrap();

    let mut rows = Vec::new();
    for tr in doc.select(&tr_sel) {
        let rank_txt = tr
            .select(&rank_sel)
            .next()
            .map(|el| clean_text(&el.text().collect::<String>()))
            .unwrap_or_default();
        let Ok(rank) = rank_txt.parse::<u32>() else {
            continue;
        };

        let mut umaban_opt = None;
        for u_el in tr.select(&umaban_sel) {
            let txt = clean_text(&u_el.text().collect::<String>());
            if let Ok(u) = txt.parse::<u32>() {
                umaban_opt = Some(u);
                break;
            }
        }
        let Some(umaban) = umaban_opt else { continue };

        let time = tr
            .select(&time_sel)
            .next()
            .map(|el| clean_text(&el.text().collect::<String>()))
            .filter(|s| !s.is_empty());

        let popularity = tr
            .select(&pop_sel)
            .next()
            .and_then(|el| clean_text(&el.text().collect::<String>()).parse::<u32>().ok());

        let win_odds = tr
            .select(&odds_sel)
            .next()
            .and_then(|el| clean_text(&el.text().collect::<String>()).parse::<f64>().ok());

        let mut last_3f = None;
        for t_el in tr.select(&bg_time_sel) {
            let txt = clean_text(&t_el.text().collect::<String>());
            if let Ok(v) = txt.parse::<f64>() {
                if (30.0..45.0).contains(&v) {
                    last_3f = Some(v);
                }
            }
        }

        let corner_last = tr
            .select(&passage_sel)
            .next()
            .map(|el| clean_text(&el.text().collect::<String>()))
            .and_then(|s| s.split('-').filter(|x| !x.is_empty()).last().and_then(|x| x.trim().parse::<u32>().ok()));

        rows.push(ResultRow {
            umaban,
            rank,
            time,
            last_3f,
            win_odds,
            popularity,
            corner_last,
        });
    }
    rows
}

/// 当日の馬場の偏り用: JRA のレース結果（馬場 "芝"/"ダ"/"障" と各馬の着順・最後のコーナー順位）。未確定なら None
pub async fn fetch_result_for_bias(client: &reqwest::Client, race_id: &str) -> Option<(String, Vec<ResultRow>)> {
    let url = format!("https://race.netkeiba.com/race/result.html?race_id={}", race_id);
    let html = fetch_html(client, &url).await.ok()?;
    let doc = Html::parse_document(&html);
    let rows = parse_result_rows(&doc);
    if rows.is_empty() {
        return None;
    }
    let data_sel = Selector::parse(".RaceData01").unwrap();
    let txt = doc.select(&data_sel).next().map(|el| el.text().collect::<String>()).unwrap_or_default();
    Some((detect_surface(&txt), rows))
}

/// 確定着順を出走馬に書き込む。fill_odds が true なら、オッズ未取得の馬に確定オッズを補う。
pub fn apply_result_rows(entries: &mut [HorseEntry], rows: &[ResultRow], fill_odds: bool) {
    for row in rows {
        if let Some(entry) = entries.iter_mut().find(|e| e.umaban == row.umaban) {
            entry.actual_rank = Some(row.rank);
            if row.time.is_some() {
                entry.actual_time = row.time.clone();
            }
            if row.last_3f.is_some() {
                entry.actual_last_3f = row.last_3f;
            }
            if fill_odds {
                if entry.win_odds.is_none() && row.win_odds.is_some() {
                    entry.win_odds = row.win_odds;
                }
                if entry.popularity.is_none() && row.popularity.is_some() {
                    entry.popularity = row.popularity;
                }
            }
        }
    }
}

/// result.html から確定着順と払戻を読む。返り値の Vec<ResultRow> が空なら未確定。
fn parse_result_and_payouts(html: &str) -> (Vec<ResultRow>, Vec<OfficialPayoutItem>) {
    let doc = Html::parse_document(html);
    let rows = parse_result_rows(&doc);

    // Parse Payout_Detail_Table
    let mut payouts = Vec::new();
    let payout_tr_sel = Selector::parse("table.Payout_Detail_Table tr").unwrap();
    let th_sel = Selector::parse("th").unwrap();
    let ul_sel = Selector::parse("td.Result ul").unwrap();
    let li_span_sel = Selector::parse("li span").unwrap();
    let div_span_sel = Selector::parse("td.Result div span").unwrap();
    let pay_span_sel = Selector::parse("td.Payout span").unwrap();
    let ninki_span_sel = Selector::parse("td.Ninki span").unwrap();
    let re_digits = Regex::new(r"(\d+)").unwrap();

    for tr in doc.select(&payout_tr_sel) {
        let bet_type = tr
            .select(&th_sel)
            .next()
            .map(|el| clean_text(&el.text().collect::<String>()))
            .unwrap_or_default();
        if bet_type.is_empty() {
            continue;
        }

        let mut combos: Vec<Vec<u32>> = Vec::new();
        let uls: Vec<_> = tr.select(&ul_sel).collect();
        if !uls.is_empty() {
            for ul in uls {
                let mut c = Vec::new();
                for sp in ul.select(&li_span_sel) {
                    let t = clean_text(&sp.text().collect::<String>());
                    if let Ok(n) = t.parse::<u32>() {
                        c.push(n);
                    }
                }
                if !c.is_empty() {
                    combos.push(c);
                }
            }
        } else {
            for sp in tr.select(&div_span_sel) {
                let t = clean_text(&sp.text().collect::<String>());
                if let Ok(n) = t.parse::<u32>() {
                    combos.push(vec![n]);
                }
            }
        }

        let mut pay_values: Vec<u32> = Vec::new();
        if let Some(pay_sp) = tr.select(&pay_span_sel).next() {
            let raw_html = pay_sp.inner_html();
            for part in Regex::new(r"(?i)<br\s*/?>").unwrap().split(&raw_html) {
                let cleaned = part.replace(',', "");
                if let Some(caps) = re_digits.captures(&cleaned) {
                    if let Ok(yen) = caps[1].parse::<u32>() {
                        pay_values.push(yen);
                    }
                }
            }
        }

        let mut ninki_values: Vec<Option<u32>> = Vec::new();
        for sp in tr.select(&ninki_span_sel) {
            let txt = clean_text(&sp.text().collect::<String>()).replace(',', "");
            let p = re_digits
                .captures(&txt)
                .and_then(|c| c[1].parse::<u32>().ok());
            ninki_values.push(p);
        }

        for (i, combo) in combos.into_iter().enumerate() {
            if let Some(&payout_yen) = pay_values.get(i) {
                let sep = if bet_type == "馬単" || bet_type == "3連単" {
                    " → "
                } else {
                    " - "
                };
                let combination_text = combo
                    .iter()
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join(sep);
                let popularity = ninki_values.get(i).copied().flatten();
                payouts.push(OfficialPayoutItem {
                    bet_type: bet_type.clone(),
                    combination: combo,
                    combination_text,
                    payout_yen,
                    popularity,
                });
            }
        }
    }

    (rows, payouts)
}

/// 確定結果と払戻だけを取得する（保存済み予想の結果照合用）。
pub async fn fetch_race_result(
    race_id: &str,
) -> Result<(Vec<ResultRow>, Vec<OfficialPayoutItem>), String> {
    let (race_id, is_nar) = extract_race_id(race_id)?;
    let base_domain = if is_nar {
        "https://nar.netkeiba.com"
    } else {
        "https://race.netkeiba.com"
    };
    let client = build_client()?;
    let result_url = format!("{}/race/result.html?race_id={}", base_domain, race_id);
    let html = fetch_html(&client, &result_url).await?;
    Ok(parse_result_and_payouts(&html))
}

/// JRA の組み合わせ馬券オッズ（馬連=4, ワイド=5, 3連複=7）。取れなかった券種は空のまま。
pub async fn fetch_jra_combo_odds(client: &reqwest::Client, race_id: &str) -> ComboOdds {
    let mut combo = ComboOdds::default();
    for (type_id, pair_len) in [("4", 2_usize), ("5", 2), ("7", 3), ("6", 2), ("8", 3)] {
        let url = format!(
            "https://race.netkeiba.com/api/api_get_jra_odds.html?pid=api_get_jra_odds&input=UTF-8&output=json&race_id={}&type={}&action=init&compress=0",
            race_id, type_id
        );
        let Ok(text) = fetch_html(client, &url).await else {
            continue;
        };
        // 馬単(6)・3連単(8) は着順に意味があるので、キーを並べ替えない
        let ordered = matches!(type_id, "6" | "8");
        let map = parse_combo_odds_json(&text, type_id, pair_len, ordered);
        match type_id {
            "4" => combo.umaren = map,
            "5" => combo.wide = map,
            "6" => combo.umatan = map,
            "8" => combo.sanrentan = map,
            _ => combo.sanrenpuku = map,
        }
    }
    combo
}

/// odds API の JSON から {"0102": ["12.3", ...]} 形式を読み、キーを "1-2" に直す。
/// ordered が false なら馬番を昇順にそろえ、true なら着順どおりのまま（馬単・3連単）
pub(crate) fn parse_combo_odds_json(
    text: &str,
    type_id: &str,
    horses: usize,
    ordered: bool,
) -> HashMap<String, f64> {
    let mut out = HashMap::new();
    let Ok(json) = serde_json::from_str::<Value>(text) else {
        return out;
    };
    let Some(obj) = json
        .get("data")
        .and_then(|d| d.get("odds"))
        .and_then(|o| o.get(type_id))
        .and_then(|v| v.as_object())
    else {
        return out;
    };
    for (k, v) in obj {
        if k.len() != horses * 2 {
            continue;
        }
        let nums: Option<Vec<u32>> = (0..horses)
            .map(|i| k[i * 2..i * 2 + 2].parse::<u32>().ok())
            .collect();
        let Some(nums) = nums else { continue };
        let odds = v
            .as_array()
            .and_then(|a| a.first())
            .and_then(|x| x.as_str())
            .and_then(|s| s.replace(',', "").parse::<f64>().ok())
            .filter(|&o| o > 0.0);
        if let Some(o) = odds {
            let key = if ordered {
                nums.iter().map(|n| n.to_string()).collect::<Vec<_>>().join("-")
            } else {
                combo_key(&nums)
            };
            out.insert(key, o);
        }
    }
    out
}

pub async fn scrape_race_data(
    input: &str,
) -> Result<(RaceInfo, Vec<HorseEntry>, Vec<OfficialPayoutItem>, ComboOdds), String> {
    let (race_id, is_nar) = extract_race_id(input)?;
    let base_domain = if is_nar {
        "https://nar.netkeiba.com"
    } else {
        "https://race.netkeiba.com"
    };

    let client = build_client()?;
    let past_url = format!("{}/race/shutuba_past.html?race_id={}", base_domain, race_id);
    let html = fetch_html(&client, &past_url).await?;

    let (mut race_info, mut entries, odds_key_map) =
        parse_shutuba_past_html(&html, &race_id, &past_url)?;

    // If shutuba_past had no entries (e.g. special race), fallback to shutuba.html
    if entries.is_empty() {
        let shutuba_url = format!("{}/race/shutuba.html?race_id={}", base_domain, race_id);
        if let Ok(s_html) = fetch_html(&client, &shutuba_url).await {
            let (r_info, s_entries) = parse_shutuba_fallback_html(&s_html, &race_id, &shutuba_url)?;
            race_info = r_info;
            entries = s_entries;
        }
    }

    if entries.is_empty() {
        return Err(format!(
            "レースID {} の出馬表データが見つかりませんでした。開催前または存在しないレースIDの可能性があります。",
            race_id
        ));
    }

    // Fetch live/official/estimated JRA odds JSON API
    let mut combo_odds = ComboOdds::default();
    if !is_nar {
        combo_odds = fetch_jra_combo_odds(&client, &race_id).await;
        let odds_map = fetch_jra_odds(&client, &race_id).await;
        for entry in &mut entries {
            let lookup_key = odds_key_map
                .get(&entry.umaban)
                .copied()
                .unwrap_or(entry.umaban);
            if let Some(&(win_o, p_min, p_max, pop)) = odds_map
                .get(&lookup_key)
                .or_else(|| odds_map.get(&entry.umaban))
            {
                if win_o.is_some() {
                    entry.win_odds = win_o;
                }
                if p_min.is_some() {
                    entry.place_odds_min = p_min;
                }
                if p_max.is_some() {
                    entry.place_odds_max = p_max;
                }
                if pop.is_some() {
                    entry.popularity = pop;
                }
            }
        }
    }

    // Check result.html for completed race verification & payouts
    let result_url = format!("{}/race/result.html?race_id={}", base_domain, race_id);
    let mut official_payouts = Vec::new();
    if let Ok(res_html) = fetch_html(&client, &result_url).await {
        let (rows, payouts) = parse_result_and_payouts(&res_html);
        if !rows.is_empty() {
            apply_result_rows(&mut entries, &rows, true);
            race_info.is_finished = true;
            official_payouts = payouts;
        }
    }

    race_info.head_count = entries.iter().filter(|e| !e.is_scratched).count() as u32;

    Ok((race_info, entries, official_payouts, combo_odds))
}

fn parse_race_header(doc: &Html, race_id: &str, source_url: &str) -> RaceInfo {
    let title_sel = Selector::parse("title").unwrap();
    let racename_sel = Selector::parse("div.RaceName, h1.RaceName").unwrap();
    let racenum_sel = Selector::parse("span.RaceNum").unwrap();
    let data01_sel = Selector::parse("div.RaceData01").unwrap();
    let data02_sel = Selector::parse("div.RaceData02").unwrap();

    let page_title = doc
        .select(&title_sel)
        .next()
        .map(|el| clean_text(&el.text().collect::<String>()))
        .unwrap_or_default();

    let re_date = Regex::new(r"(\d{4}年\d{1,2}月\d{1,2}日)").unwrap();
    let date_str = re_date
        .captures(&page_title)
        .map(|c| c[1].to_string())
        .unwrap_or_else(|| format!("{}年", &race_id[0..4]));

    let racename_el = doc.select(&racename_sel).next();
    let raw_race_name = racename_el
        .map(|el| {
            el.text()
                .next()
                .map(clean_text)
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| clean_text(&el.text().collect::<String>()))
        })
        .unwrap_or_else(|| "レース".to_string());

    let grade = racename_el.and_then(|el| parse_grade_from_classes(&el.inner_html()));

    let race_num = doc
        .select(&racenum_sel)
        .next()
        .map(|el| clean_text(&el.text().collect::<String>()))
        .unwrap_or_else(|| format!("{}R", race_id[10..12].trim_start_matches('0')));

    let d01_el = doc.select(&data01_sel).next();
    let d01_txt = d01_el
        .map(|el| clean_text(&el.text().collect::<String>()))
        .unwrap_or_default();
    let d01_html = d01_el.map(|el| el.inner_html()).unwrap_or_default();

    let re_time = Regex::new(r"(\d{1,2}:\d{2})").unwrap();
    let start_time = re_time
        .captures(&d01_txt)
        .map(|c| c[1].to_string())
        .unwrap_or_else(|| "--:--".to_string());

    let surface = if is_jump_race(&raw_race_name, grade.as_deref()) {
        "障".to_string()
    } else {
        detect_surface(&d01_txt)
    };

    let re_dist = Regex::new(r"(\d{3,4})m").unwrap();
    let distance = re_dist
        .captures(&d01_txt)
        .and_then(|c| c[1].parse::<u32>().ok())
        .unwrap_or(1600);

    let direction = if d01_txt.contains('左') {
        "左".to_string()
    } else if d01_txt.contains('右') {
        "右".to_string()
    } else if d01_txt.contains('直') {
        "直".to_string()
    } else {
        "右".to_string()
    };

    let weather = parse_weather_from_classes(&d01_html);
    let track_cond = if d01_txt.contains("不良") || d01_txt.contains("不") {
        "不".to_string()
    } else if d01_txt.contains("稍") {
        "稍".to_string()
    } else if d01_txt.contains("重") {
        "重".to_string()
    } else {
        "良".to_string()
    };

    let d02_txt = doc
        .select(&data02_sel)
        .next()
        .map(|el| clean_text(&el.text().collect::<String>()))
        .unwrap_or_default();

    let venue = [
        "札幌", "函館", "福島", "新潟", "東京", "中山", "中京", "京都", "阪神", "小倉",
        "門別", "盛岡", "水沢", "浦和", "船橋", "大井", "川崎", "金沢", "笠松", "名古屋",
        "園田", "姫路", "高知", "佐賀",
    ]
    .iter()
    .find(|&&v| d02_txt.contains(v) || page_title.contains(v))
    .map(|&s| s.to_string())
    .unwrap_or_else(|| venue_code_to_name(&race_id[4..6]).to_string());

    RaceInfo {
        race_id: race_id.to_string(),
        race_name: raw_race_name,
        race_num,
        date_str,
        venue,
        grade,
        start_time,
        surface,
        distance,
        direction,
        weather,
        track_cond,
        head_count: 0,
        is_finished: false,
        source_url: source_url.to_string(),
    }
}

fn parse_shutuba_past_html(
    html: &str,
    race_id: &str,
    source_url: &str,
) -> Result<(RaceInfo, Vec<HorseEntry>, HashMap<u32, u32>), String> {
    let doc = Html::parse_document(html);
    let race_info = parse_race_header(&doc, race_id, source_url);

    let table_sel = Selector::parse("#sort_table, table.Shutuba_Past5_Table").unwrap();
    let tr_sel = Selector::parse("tr.HorseList").unwrap();
    let td_sel = Selector::parse("td").unwrap();
    let h01_sel = Selector::parse(".Horse01").unwrap();
    let h02_a_sel = Selector::parse(".Horse02 a").unwrap();
    let h03_sel = Selector::parse(".Horse03").unwrap();
    let h04_sel = Selector::parse(".Horse04").unwrap();
    let h05_sel = Selector::parse(".Horse05").unwrap();
    let h06_sel = Selector::parse(".Horse06").unwrap();
    let kyakusitu_sel = Selector::parse("span.kyakusitu, div.Type span").unwrap();
    let weight_sel = Selector::parse(".Horse07 div.Weight").unwrap();
    let popular_sel = Selector::parse(".Horse07 div.Popular").unwrap();
    let odds_span_sel = Selector::parse("span[id^='odds-1_']").unwrap();
    let jockey_td_sel = Selector::parse("td.Jockey").unwrap();
    let barei_sel = Selector::parse("span.Barei").unwrap();
    let jockey_a_sel = Selector::parse("a[href*='/jockey/']").unwrap();
    let span_sel = Selector::parse("span").unwrap();
    let past_td_sel = Selector::parse("td.Past").unwrap();

    let re_horse_id = Regex::new(r"/horse/(\d+)").unwrap();
    let re_jockey_id = Regex::new(r"/jockey/(?:result/recent/)?(\w+)").unwrap();
    let re_weight = Regex::new(r"(\d{3})kg(?:\(([+-]?\d+)\))?").unwrap();
    let re_inline_odds = Regex::new(r"(\d+\.\d+)").unwrap();
    let re_inline_pop = Regex::new(r"\((\d+)人気\)").unwrap();

    let mut entries = Vec::new();
    let mut odds_key_map = HashMap::new();

    let Some(main_table) = doc.select(&table_sel).next() else {
        return Ok((race_info, entries, odds_key_map));
    };

    for tr in main_table.select(&tr_sel) {
        let tr_class = tr.value().attr("class").unwrap_or("");
        let tr_html = tr.inner_html();
        let is_scratched = tr_class.contains("Cancel")
            || tr_html.contains("Cancel_NoData")
            || tr_html.contains("Cancel_Txt");

        let tds: Vec<_> = tr.select(&td_sel).collect();
        if tds.len() < 3 {
            continue;
        }

        let provisional_waku = (((entries.len() / 2) % 8) + 1) as u32;
        let waku = clean_text(&tds[0].text().collect::<String>())
            .parse::<u32>()
            .unwrap_or(provisional_waku);
        let umaban = clean_text(&tds[1].text().collect::<String>())
            .parse::<u32>()
            .unwrap_or((entries.len() + 1) as u32);

        if let Some(odds_sp) = tr.select(&odds_span_sel).next() {
            if let Some(id_attr) = odds_sp.value().attr("id") {
                if let Some(num_str) = id_attr.strip_prefix("odds-1_") {
                    if let Ok(k) = num_str.parse::<u32>() {
                        odds_key_map.insert(umaban, k);
                    }
                }
            }
        }

        let Some(h02_a) = tr.select(&h02_a_sel).next() else {
            continue;
        };
        let horse_name = clean_text(&h02_a.text().collect::<String>());
        if horse_name.is_empty() {
            continue;
        }
        let horse_id = h02_a
            .value()
            .attr("href")
            .and_then(|h| re_horse_id.captures(h))
            .map(|c| c[1].to_string())
            .unwrap_or_default();

        let sire = tr
            .select(&h01_sel)
            .next()
            .map(|el| clean_text(&el.text().collect::<String>()))
            .unwrap_or_default();
        let dam = tr
            .select(&h03_sel)
            .next()
            .map(|el| clean_text(&el.text().collect::<String>()))
            .unwrap_or_default();
        let broodmare_sire = tr
            .select(&h04_sel)
            .next()
            .map(|el| {
                clean_text(&el.text().collect::<String>())
                    .trim_matches(|c| c == '(' || c == ')')
                    .to_string()
            })
            .unwrap_or_default();
        let trainer_name = tr
            .select(&h05_sel)
            .next()
            .map(|el| clean_text(&el.text().collect::<String>()))
            .unwrap_or_default();

        let running_style = tr
            .select(&kyakusitu_sel)
            .next()
            .map(|el| clean_text(&el.text().collect::<String>()))
            .unwrap_or_default();

        let interval_text = tr
            .select(&h06_sel)
            .next()
            .map(|el| {
                clean_text(&el.text().collect::<String>())
                    .replace(&running_style, "")
                    .trim()
                    .to_string()
            })
            .unwrap_or_default();

        let weight_txt = tr
            .select(&weight_sel)
            .next()
            .map(|el| clean_text(&el.text().collect::<String>()))
            .unwrap_or_default();
        let (body_weight, weight_diff) = if let Some(caps) = re_weight.captures(&weight_txt) {
            let bw = caps.get(1).and_then(|m| m.as_str().parse::<u32>().ok());
            let wd = caps.get(2).and_then(|m| m.as_str().parse::<i32>().ok());
            (bw, wd)
        } else {
            (None, None)
        };

        // Parse inline odds & popularity if present in .Horse07 div.Popular (used by NAR)
        let mut inline_win_odds = None;
        let mut inline_popularity = None;
        if let Some(pop_el) = tr.select(&popular_sel).next() {
            let pop_txt = clean_text(&pop_el.text().collect::<String>());
            inline_win_odds = re_inline_odds
                .captures(&pop_txt)
                .and_then(|c| c[1].parse::<f64>().ok())
                .filter(|&v| v > 0.0);
            inline_popularity = re_inline_pop
                .captures(&pop_txt)
                .and_then(|c| c[1].parse::<u32>().ok())
                .filter(|&p| p > 0);
        }

        let mut sex_age = String::new();
        let mut jockey_name = String::new();
        let mut jockey_id = String::new();
        let mut carried_weight = 56.0;

        if let Some(j_td) = tr.select(&jockey_td_sel).next() {
            let mut raw_barei_full = String::new();
            if let Some(b_el) = j_td.select(&barei_sel).next() {
                raw_barei_full = clean_text(&b_el.text().collect::<String>());
                sex_age = raw_barei_full.chars().take(2).collect();
            }
            if let Some(j_a) = j_td.select(&jockey_a_sel).next() {
                jockey_name = clean_text(&j_a.text().collect::<String>());
                jockey_id = j_a
                    .value()
                    .attr("href")
                    .and_then(|h| re_jockey_id.captures(h))
                    .map(|c| c[1].to_string())
                    .unwrap_or_default();
            }
            for sp in j_td.select(&span_sel) {
                let t = clean_text(&sp.text().collect::<String>());
                if let Ok(w) = t.parse::<f64>() {
                    if (45.0..65.0).contains(&w) {
                        carried_weight = w;
                    }
                }
            }
            // Fallback for NAR where jockey name is direct text inside td.Jockey
            if jockey_name.is_empty() {
                for text_node in j_td.text() {
                    let t = clean_text(text_node);
                    if !t.is_empty()
                        && t != raw_barei_full
                        && t != sex_age
                        && t.parse::<f64>().is_err()
                    {
                        jockey_name = t;
                        break;
                    }
                }
            }
        }

        let mut past_runs = Vec::new();
        for p_td in tr.select(&past_td_sel) {
            if let Some(run) = parse_past_run(p_td) {
                past_runs.push(run);
            }
        }

        // infer running style from past runs corner passage if missing
        let inferred_style = if !running_style.is_empty() {
            running_style
        } else {
            infer_running_style_from_past(&past_runs)
        };

        entries.push(HorseEntry {
            waku,
            umaban,
            horse_id,
            horse_name,
            sex_age,
            carried_weight,
            jockey_name,
            jockey_id,
            trainer_name,
            sire,
            dam,
            broodmare_sire,
            running_style: inferred_style,
            interval_text,
            body_weight,
            weight_diff,
            win_odds: inline_win_odds,
            place_odds_min: None,
            place_odds_max: None,
            popularity: inline_popularity,
            is_scratched,
            past_runs,
            actual_rank: None,
            actual_time: None,
            actual_last_3f: None,
        });
    }

    Ok((race_info, entries, odds_key_map))
}

fn infer_running_style_from_past(past_runs: &[PastRun]) -> String {
    if past_runs.is_empty() {
        return "先".to_string();
    }
    let mut sum_ratio = 0.0;
    let mut count = 0;
    for r in past_runs {
        let first_corner = r
            .passage
            .split('-')
            .next()
            .and_then(|s| s.parse::<f64>().ok());
        let heads = r.head_count.unwrap_or(16) as f64;
        if let Some(pos) = first_corner {
            sum_ratio += pos / heads.max(1.0);
            count += 1;
        }
    }
    if count == 0 {
        return "先".to_string();
    }
    let avg = sum_ratio / (count as f64);
    if avg <= 0.15 {
        "逃".to_string()
    } else if avg <= 0.40 {
        "先".to_string()
    } else if avg <= 0.72 {
        "差".to_string()
    } else {
        "追".to_string()
    }
}

fn parse_shutuba_fallback_html(
    html: &str,
    race_id: &str,
    source_url: &str,
) -> Result<(RaceInfo, Vec<HorseEntry>), String> {
    let doc = Html::parse_document(html);
    let race_info = parse_race_header(&doc, race_id, source_url);

    let tr_sel = Selector::parse("table.Shutuba_Table tr.HorseList").unwrap();
    let waku_sel = Selector::parse("td[class^='Waku']").unwrap();
    let umaban_sel = Selector::parse("td[class^='Umaban']").unwrap();
    let name_sel = Selector::parse("span.HorseName a").unwrap();
    let barei_sel = Selector::parse("td.Barei").unwrap();
    let weight_carried_sel = Selector::parse("td.Txt_C").unwrap();
    let jockey_sel = Selector::parse("td.Jockey a").unwrap();
    let trainer_sel = Selector::parse("td.Trainer a").unwrap();

    let mut entries = Vec::new();
    for tr in doc.select(&tr_sel) {
        let Some(name_a) = tr.select(&name_sel).next() else {
            continue;
        };
        let horse_name = clean_text(&name_a.text().collect::<String>());
        if horse_name.is_empty() {
            continue;
        }
        let waku = tr
            .select(&waku_sel)
            .next()
            .and_then(|el| clean_text(&el.text().collect::<String>()).parse::<u32>().ok())
            .unwrap_or(1);
        let umaban = tr
            .select(&umaban_sel)
            .next()
            .and_then(|el| clean_text(&el.text().collect::<String>()).parse::<u32>().ok())
            .unwrap_or((entries.len() + 1) as u32);
        let sex_age = tr
            .select(&barei_sel)
            .next()
            .map(|el| clean_text(&el.text().collect::<String>()))
            .unwrap_or_default();
        let jockey_name = tr
            .select(&jockey_sel)
            .next()
            .map(|el| clean_text(&el.text().collect::<String>()))
            .unwrap_or_default();
        let trainer_name = tr
            .select(&trainer_sel)
            .next()
            .map(|el| clean_text(&el.text().collect::<String>()))
            .unwrap_or_default();
        let mut carried_weight = 56.0;
        for td in tr.select(&weight_carried_sel) {
            if let Ok(w) = clean_text(&td.text().collect::<String>()).parse::<f64>() {
                if (45.0..65.0).contains(&w) {
                    carried_weight = w;
                    break;
                }
            }
        }

        entries.push(HorseEntry {
            waku,
            umaban,
            horse_id: String::new(),
            horse_name,
            sex_age,
            carried_weight,
            jockey_name,
            jockey_id: String::new(),
            trainer_name,
            sire: String::new(),
            dam: String::new(),
            broodmare_sire: String::new(),
            running_style: "先".to_string(),
            interval_text: String::new(),
            body_weight: None,
            weight_diff: None,
            win_odds: None,
            place_odds_min: None,
            place_odds_max: None,
            popularity: None,
            is_scratched: tr.value().attr("class").unwrap_or("").contains("Cancel"),
            past_runs: Vec::new(),
            actual_rank: None,
            actual_time: None,
            actual_last_3f: None,
        });
    }

    Ok((race_info, entries))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horse_results_are_read_by_header_name() {
        let html = r#"<table class="db_h_race_results"><thead><tr>
            <th>日付</th><th>開催</th><th>レース名</th><th>頭数</th><th>枠番</th><th>馬番</th><th>オッズ</th><th>人気</th>
            <th>着順</th><th>騎手</th><th>斤量</th><th>距離</th><th>馬場</th><th>馬場指数</th><th>タイム</th><th>着差</th>
            <th>通過</th><th>ペース</th><th>上り</th><th>馬体重</th></tr></thead><tbody>
            <tr><td>2023/10/15</td><td>2京都5</td><td><a href="/race/202308020511/">秋華賞(GI)</a></td><td>18</td><td>3</td><td>6</td>
            <td>1.1</td><td>1</td><td>1</td><td>川田将雅</td><td>55</td><td>芝2000</td><td>稍</td><td>-5</td><td>2:01.1</td><td>-0.1</td>
            <td>6-7-7-3</td><td>36.4-34.0</td><td class="rank_2 ">33.6</td><td>476(+10)</td></tr></tbody></table>"#;
        let runs = parse_horse_results(html);
        assert_eq!(runs.len(), 1);
        let r = &runs[0];
        assert_eq!(r.race_id, "202308020511");
        assert_eq!((r.surface.as_str(), r.distance), ("芝", Some(2000)));
        assert_eq!((r.race_first3f, r.race_last3f), (Some(36.4), Some(34.0)));
        assert_eq!((r.last3f, r.last3f_rank), (Some(33.6), Some(2)));
        assert_eq!(r.time_sec, Some(121.1));
        assert_eq!(r.track_index, Some(-5));
        assert_eq!(r.body_weight, Some(476));
    }

    #[test]
    fn jump_races_are_detected_before_turf() {
        // netkeiba の実際の表記（202306050710 中山大障害）
        assert_eq!(detect_surface("15:05発走 / 障4100m 芝 / 良"), "障");
        assert_eq!(detect_surface("障3170(芝 内-外) 3:38.7"), "障");
        assert_eq!(detect_surface("芝2400m 左 C / 良"), "芝");
        assert_eq!(detect_surface("ダ1800m"), "ダ");
        // 枠順確定前の障害戦は "芝2750m" としか出ない（202603030404 福島4R）
        assert!(is_jump_race("3歳以上障害未勝利", None));
        assert!(is_jump_race("東京ハイJ", Some("J・GII")));
        assert!(!is_jump_race("2歳未勝利", None));
        assert!(!is_jump_race("ジャパンC", Some("GI")));
    }

    #[test]
    fn race_name_excludes_grade_icon() {
        let html = Html::parse_fragment(
            r#"<div class="Data02"><a href="/race/1">毎日王冠 <span class="Icon_GradeType Icon_GradeType2">GII</span> </a></div>
               <div class="Data02"><a href="/race/2">中山大障害 <span class="Icon_GradeType Icon_GradeType10">JGI</span> </a></div>"#,
        );
        let sel = Selector::parse("div.Data02 a").unwrap();
        let names: Vec<String> = html.select(&sel).map(race_name_from_link).collect();
        assert_eq!(names, vec!["毎日王冠", "中山大障害"]);
    }

    #[test]
    fn combo_odds_json_is_keyed_by_sorted_umaban() {
        let json = r#"{"status":"result","data":{"odds":{"7":{"010203":["7.4","0.0","4"],"010205":["1,205.2","0.0","1"],"0102":["1.0","0","1"]}}}}"#;
        let m = parse_combo_odds_json(json, "7", 3, false);
        assert_eq!(m.get("1-2-3"), Some(&7.4));
        assert_eq!(m.get("1-2-5"), Some(&1205.2));
        assert_eq!(m.len(), 2);
    }
}


/// db.netkeiba.com の馬の競走成績ページ（/horse/result/<id>/）から全走を読む。
/// 列は見出しの名前で探す（列の並びが変わっても壊れにくくするため）。
pub fn parse_horse_results(html: &str) -> Vec<HorseRun> {
    let doc = Html::parse_document(html);
    let table_sel = Selector::parse("table.db_h_race_results").unwrap();
    let th_sel = Selector::parse("thead th, tr th").unwrap();
    let tr_sel = Selector::parse("tbody tr").unwrap();
    let td_sel = Selector::parse("td").unwrap();
    let a_sel = Selector::parse("a").unwrap();
    let Some(table) = doc.select(&table_sel).next() else {
        return Vec::new();
    };
    let headers: Vec<String> = table
        .select(&th_sel)
        .map(|h| h.text().collect::<String>().split_whitespace().collect::<String>())
        .collect();
    let col = |name: &str| headers.iter().position(|h| h == name);
    let (c_date, c_venue, c_name, c_heads, c_waku, c_umaban) =
        (col("日付"), col("開催"), col("レース名"), col("頭数"), col("枠番"), col("馬番"));
    let (c_odds, c_pop, c_rank, c_jockey, c_weight, c_dist) =
        (col("オッズ"), col("人気"), col("着順"), col("騎手"), col("斤量"), col("距離"));
    let (c_cond, c_tidx, c_time, c_margin, c_pass, c_pace, c_last, c_body) = (
        col("馬場"),
        col("馬場指数"),
        col("タイム"),
        col("着差"),
        col("通過"),
        col("ペース"),
        col("上り"),
        col("馬体重"),
    );
    let re_dist = Regex::new(r"^(芝|ダ|障)(\d{3,4})").unwrap();
    let re_race = Regex::new(r"/race/(\w+)/?").unwrap();
    let re_body = Regex::new(r"^(\d{3})").unwrap();
    let re_rank = Regex::new(r"rank_(\d)").unwrap();

    let mut out = Vec::new();
    for tr in table.select(&tr_sel) {
        let tds: Vec<ElementRef> = tr.select(&td_sel).collect();
        if tds.len() < 10 {
            continue;
        }
        let text = |c: Option<usize>| -> String {
            c.and_then(|i| tds.get(i))
                .map(|td| clean_text(&td.text().collect::<String>()))
                .unwrap_or_default()
        };
        let num = |c: Option<usize>| -> Option<f64> { text(c).replace(',', "").parse::<f64>().ok() };
        let int = |c: Option<usize>| -> Option<u32> { text(c).parse::<u32>().ok() };
        let race_id = c_name
            .and_then(|i| tds.get(i))
            .and_then(|td| td.select(&a_sel).next())
            .and_then(|a| a.value().attr("href"))
            .and_then(|h| re_race.captures(h).map(|c| c[1].to_string()))
            .unwrap_or_default();
        let dist_txt = text(c_dist);
        let (surface, distance) = match re_dist.captures(&dist_txt) {
            Some(c) => (c[1].to_string(), c[2].parse().ok()),
            None => (String::new(), None),
        };
        let pace = text(c_pace);
        let mut pp = pace.split('-').map(|x| x.trim().parse::<f64>().ok());
        let (f3, l3) = (pp.next().flatten(), pp.next().flatten());
        let last_rank = c_last
            .and_then(|i| tds.get(i))
            .and_then(|td| td.value().attr("class"))
            .and_then(|cl| re_rank.captures(cl).and_then(|c| c[1].parse::<u32>().ok()));
        out.push(HorseRun {
            date: text(c_date),
            race_id,
            venue: text(c_venue),
            race_name: text(c_name),
            head_count: int(c_heads),
            waku: int(c_waku),
            umaban: int(c_umaban),
            odds: num(c_odds),
            popularity: int(c_pop),
            finish_rank: int(c_rank),
            jockey: text(c_jockey),
            carried_weight: num(c_weight),
            surface,
            distance,
            track_cond: text(c_cond),
            track_index: text(c_tidx).parse::<i32>().ok(),
            time_sec: parse_time_to_seconds(&text(c_time)),
            margin: num(c_margin),
            passage: text(c_pass),
            race_first3f: f3.filter(|v| *v > 0.0),
            race_last3f: l3.filter(|v| *v > 0.0),
            last3f: num(c_last),
            last3f_rank: last_rank,
            body_weight: re_body.captures(&text(c_body)).and_then(|c| c[1].parse().ok()),
        });
    }
    out
}

pub async fn fetch_horse_results(horse_id: &str) -> Result<Vec<HorseRun>, String> {
    let client = build_client()?;
    let url = format!("https://db.netkeiba.com/horse/result/{}/", horse_id);
    let html = fetch_html(&client, &url).await?;
    Ok(parse_horse_results(&html))
}

// ---- netkeiba の結果ページから JRA-VAN と同じ形のレースを作る（DB の更新までの成績を補うため） ----

/// 全角数字を半角に
fn ascii_digits(s: &str) -> String {
    s.chars()
        .map(|c| if ('０'..='９').contains(&c) { char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap() } else { c })
        .collect()
}

/// "1:59.0" / "59.0" → 秒
fn time_to_sec(s: &str) -> Option<f64> {
    let s = s.trim();
    match s.split_once(':') {
        Some((m, r)) => Some(m.parse::<f64>().ok()? * 60.0 + r.parse::<f64>().ok()?),
        None => s.parse().ok(),
    }
}

/// netkeiba のコース表記から JRA-VAN のトラックコード（主なものだけ。2 周・内→外などは近いものに寄せる）
fn track_code_of(surface: &str, txt: &str) -> i64 {
    let outer = txt.contains('外');
    let straight = txt.contains("直線");
    match (surface, straight, txt.contains('左'), outer) {
        ("芝", true, _, _) => 10,
        ("芝", _, true, false) => 11,
        ("芝", _, true, true) => 12,
        ("芝", _, false, false) => 17,
        ("芝", _, false, true) => 18,
        ("ダ", true, _, _) => 29,
        ("ダ", _, true, _) => 23,
        ("ダ", _, false, _) => 24,
        _ => 0,
    }
}

/// クラス表記 → 競走条件コード（新馬・未勝利 703、1勝 005、2勝 010、3勝 016、オープン 999）
fn condition_code_of(txt: &str) -> String {
    let t = ascii_digits(txt);
    if t.contains("新馬") || t.contains("未勝利") {
        "703"
    } else if t.contains("1勝") || t.contains("500万") {
        "005"
    } else if t.contains("2勝") || t.contains("1000万") {
        "010"
    } else if t.contains("3勝") || t.contains("1600万") {
        "016"
    } else {
        "999"
    }
    .to_string()
}

fn going_code(txt: &str) -> String {
    if txt.contains("不良") {
        "4"
    } else if txt.contains("稍") {
        "2"
    } else if txt.contains('重') {
        "3"
    } else if txt.contains('良') {
        "1"
    } else {
        ""
    }
    .to_string()
}

/// netkeiba の結果ページ（確定後）を JvRace にする。データ区分は 'N'（JRA-VAN の確定成績が来たら上書きされる）
pub async fn fetch_result_as_jvrace(client: &reqwest::Client, race_id: &str, date: &str) -> Option<crate::jvdata::JvRace> {
    let url = format!("https://race.netkeiba.com/race/result.html?race_id={}", race_id);
    let html = fetch_html(client, &url).await.ok()?;
    parse_result_as_jvrace(&html, race_id, date)
}

pub fn parse_result_as_jvrace(html: &str, race_id: &str, date: &str) -> Option<crate::jvdata::JvRace> {
    use crate::jvdata::{JvRace, JvRunner};
    let doc = Html::parse_document(html);
    let text_of = |sel: &str| -> String {
        let s = Selector::parse(sel).unwrap();
        doc.select(&s).next().map(|e| clean_text(&e.text().collect::<String>())).unwrap_or_default()
    };
    let d1 = text_of(".RaceData01");
    let d2 = text_of(".RaceData02");
    let surface = detect_surface(&d1);
    if surface == "障" || d1.is_empty() {
        return None;
    }
    let distance: u32 = Regex::new(r"(\d{3,4})m").unwrap().captures(&d1).and_then(|c| c[1].parse().ok())?;
    let start_time = Regex::new(r"(\d{1,2}):(\d{2})").unwrap().captures(&d1).map(|c| format!("{:0>2}{}", &c[1], &c[2])).unwrap_or_default();
    // 馬場状態は .RaceData01 の .Item03（芝）か .Item04（ダート）。後ろに風の注記が続くことがある
    let going = going_code(&format!("{} {}", text_of(".RaceData01 .Item03"), text_of(".RaceData01 .Item04")));
    let debut = ascii_digits(&d2).contains("新馬");
    let course_txt = d1.split('/').nth(1).unwrap_or("");
    let head: String = html.find("RaceName").map(|i| html[i..].chars().take(3000).collect()).unwrap_or_default();
    let grade = match parse_grade_from_classes(&head).as_deref() {
        Some("GI") => "A",
        Some("GII") => "B",
        Some("GIII") => "C",
        Some("L") => "L",
        _ => "",
    };

    // ラップ: 1 行目が通過タイム（累計）、見出しが距離
    let th_sel = Selector::parse("table.Race_HaronTime tr.Header th").unwrap();
    let cum_sel = Selector::parse("table.Race_HaronTime tr.HaronTime").unwrap();
    let td_sel = Selector::parse("td").unwrap();
    let marks: Vec<u32> = doc.select(&th_sel).filter_map(|e| clean_text(&e.text().collect::<String>()).trim_end_matches('m').parse().ok()).collect();
    let rows_t: Vec<Vec<String>> = doc.select(&cum_sel).map(|tr| tr.select(&td_sel).map(|e| clean_text(&e.text().collect::<String>())).collect()).collect();
    let cum: Vec<Option<f64>> = rows_t.first().map(|r| r.iter().map(|x| time_to_sec(x)).collect()).unwrap_or_default();
    let laps: Vec<f64> = rows_t.get(1).map(|r| r.iter().filter_map(|x| x.parse().ok()).collect()).unwrap_or_default();
    let at = |m: u32| marks.iter().position(|x| *x == m).and_then(|i| cum.get(i).copied().flatten());
    // 前 3F は最初の 3 区間の合計（JRA-VAN と同じ。端数の距離は最初の区間が短い）
    let first3f = if laps.len() >= 3 { Some(((laps[0] + laps[1] + laps[2]) * 10.0).round() / 10.0) } else { None };
    let last3f = match (at(distance), at(distance.saturating_sub(600))) {
        (Some(t), Some(b)) => Some(((t - b) * 10.0).round() / 10.0),
        _ => None,
    };

    // 各馬
    let tr_sel = Selector::parse("#All_Result_Table tr.HorseList, table.RaceTable01 tr.HorseList").unwrap();
    let a_sel = Selector::parse("a").unwrap();
    let td_any = Selector::parse("td").unwrap();
    let mut runners: Vec<JvRunner> = Vec::new();
    for tr in doc.select(&tr_sel) {
        let tds: Vec<_> = tr.select(&td_any).collect();
        let cell = |cls: &str| -> String {
            tds.iter()
                .find(|e| e.value().attr("class").map(|c| c.split_whitespace().any(|x| x == cls)).unwrap_or(false))
                .map(|e| clean_text(&e.text().collect::<String>()))
                .unwrap_or_default()
        };
        let link_id = |kind: &str| -> String {
            tr.select(&a_sel)
                .filter_map(|a| a.value().attr("href"))
                .find(|h| h.contains(&format!("/{}/", kind)))
                .and_then(|h| h.trim_end_matches('/').rsplit('/').next().map(|s| s.to_string()))
                .unwrap_or_default()
        };
        let rank_txt = cell("Result_Num");
        let (rank, abnormal) = match rank_txt.parse::<u32>() {
            Ok(r) => (Some(r), "0".to_string()),
            Err(_) if rank_txt.contains("取") => (None, "1".to_string()),
            Err(_) if rank_txt.contains("除") => (None, "2".to_string()),
            Err(_) => (None, "4".to_string()),
        };
        let umaban: u32 = tds.iter().filter(|e| e.value().attr("class").map(|c| c.contains("Num") && c.contains("Txt_C")).unwrap_or(false)).find_map(|e| clean_text(&e.text().collect::<String>()).parse().ok()).unwrap_or(0);
        let waku: u32 = tds.iter().find(|e| e.value().attr("class").map(|c| c.contains("Waku")).unwrap_or(false)).and_then(|e| clean_text(&e.text().collect::<String>()).parse().ok()).unwrap_or(0);
        let horse_id = link_id("horse");
        if horse_id.is_empty() || umaban == 0 {
            continue;
        }
        let sex_age = tds.iter().find(|e| e.value().attr("class") == Some("Horse_Info Txt_C")).map(|e| clean_text(&e.text().collect::<String>())).unwrap_or_default();
        let sex = match sex_age.chars().next() {
            Some('牡') => "1",
            Some('牝') => "2",
            _ => "3",
        };
        let age: u32 = sex_age.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse().unwrap_or(0);
        let passage: Vec<u32> = cell("PassageRate").split('-').filter_map(|x| x.trim().parse().ok()).collect();
        let mut corners = [None; 4];
        for (i, p) in passage.iter().rev().enumerate().take(4) {
            corners[3 - i] = Some(*p);
        }
        let weight_txt = cell("Weight");
        let wcap = Regex::new(r"(\d+)\(([+-]?\d+)\)").unwrap().captures(&weight_txt);
        let times: Vec<String> = tds.iter().filter(|e| e.value().attr("class").map(|c| c.split_whitespace().next() == Some("Time")).unwrap_or(false)).map(|e| clean_text(&e.text().collect::<String>())).collect();
        let last3f_h = times.iter().filter_map(|t| t.parse::<f64>().ok()).find(|v| (30.0..45.0).contains(v));
        runners.push(JvRunner {
            kubun: 'N',
            waku,
            umaban,
            horse_id,
            name: tr.select(&a_sel).find(|a| a.value().attr("href").map(|h| h.contains("/horse/")).unwrap_or(false)).map(|a| clean_text(&a.text().collect::<String>())).unwrap_or_default(),
            sex: sex.into(),
            age,
            trainer: link_id("trainer"),
            owner: String::new(),
            carried_weight: cell("Jockey_Info").parse().ok(),
            blinker: false,
            jockey: link_id("jockey"),
            jockey_name: cell("Jockey"),
            apprentice: String::new(),
            body_weight: wcap.as_ref().and_then(|c| c[1].parse().ok()),
            // 新馬戦は前走がないので増減なし（JRA-VAN も空）
            weight_diff: if debut { None } else { wcap.as_ref().and_then(|c| c[2].parse().ok()) },
            abnormal,
            rank,
            time_sec: times.first().and_then(|t| time_to_sec(t)).filter(|v| *v > 40.0),
            corners,
            win_odds: tds.iter().find(|e| e.value().attr("class").map(|c| c.contains("Odds") && c.contains("Txt_R")).unwrap_or(false)).and_then(|e| clean_text(&e.text().collect::<String>()).parse().ok()),
            popularity: tds.iter().find(|e| e.value().attr("class").map(|c| c.contains("Odds") && c.contains("Txt_C")).unwrap_or(false)).and_then(|e| clean_text(&e.text().collect::<String>()).parse().ok()),
            last3f: last3f_h,
            last4f: None,
            time_diff: None,
            mining_kubun: String::new(),
            mining_time: None,
            mining_rank: None,
            style: 0,
        });
    }
    if runners.is_empty() {
        return None;
    }
    // 1 着とのタイム差（1 着は 2 着との差を負で）、脚質（最初のコーナーの位置）
    let n = runners.iter().filter(|h| h.rank.is_some()).count().max(1) as f64;
    let t_of = |k: u32| runners.iter().find(|h| h.rank == Some(k)).and_then(|h| h.time_sec);
    let (t1, t2) = (t_of(1), t_of(2));
    for h in runners.iter_mut() {
        h.time_diff = match (h.rank, h.time_sec, t1, t2) {
            (Some(1), Some(_), Some(a), Some(b)) => Some(((a - b) * 10.0).round() / 10.0),
            (Some(_), Some(t), Some(a), _) => Some(((t - a) * 10.0).round() / 10.0),
            _ => None,
        };
        h.style = match h.corners.iter().flatten().next() {
            Some(1) => 1,
            Some(p) if (*p as f64) / n <= 1.0 / 3.0 => 2,
            Some(p) if (*p as f64) / n <= 2.0 / 3.0 => 3,
            Some(_) => 4,
            None => 0,
        };
    }
    Some(JvRace {
        race_id: race_id.to_string(),
        date: date.to_string(),
        kubun: 'N',
        venue_code: race_id.get(4..6).unwrap_or("").to_string(),
        race_name: text_of(".RaceName"),
        grade: grade.into(),
        race_type: String::new(),
        condition_min: condition_code_of(&d2),
        distance,
        track_code: track_code_of(&surface, course_txt),
        surface: surface.clone(),
        start_time,
        runners_count: runners.len() as u32,
        weather: String::new(),
        turf_cond: if surface == "芝" { going.clone() } else { "0".into() },
        dirt_cond: if surface == "ダ" { going } else { "0".into() },
        laps,
        first3f,
        last3f,
        runners,
    })
}
