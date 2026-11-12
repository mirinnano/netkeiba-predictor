use crate::models::{
    HorseEntry, KaisaiCalendarResponse, KaisaiDateItem, OfficialPayoutItem, PastRun, RaceInfo,
    RaceSummary, VenueRaces,
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

            let surface = if long_txt.contains('芝') {
                "芝".to_string()
            } else if long_txt.contains('ダ') {
                "ダ".to_string()
            } else if long_txt.contains('障') {
                "障".to_string()
            } else {
                "芝".to_string()
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
    let race_name = d02_el
        .map(|el| {
            clean_text(&el.text().collect::<String>())
                .replace("GI", "")
                .replace("GII", "")
                .replace("GIII", "")
                .trim()
                .to_string()
        })
        .unwrap_or_default();
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

    let surface = if d05_txt.contains('芝') {
        "芝".to_string()
    } else if d05_txt.contains('ダ') {
        "ダ".to_string()
    } else if d05_txt.contains('障') {
        "障".to_string()
    } else {
        "芝".to_string()
    };

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

async fn fetch_jra_odds(
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

fn parse_result_and_payouts(
    html: &str,
    entries: &mut [HorseEntry],
) -> (bool, Vec<OfficialPayoutItem>) {
    let doc = Html::parse_document(html);
    let tr_sel = Selector::parse("#All_Result_Table tr.HorseList, table.RaceTable01 tr.HorseList").unwrap();
    let rank_sel = Selector::parse("td.Result_Num div.Rank, td.Result_Num").unwrap();
    let umaban_sel = Selector::parse("td.Num.Txt_C div, td.Num.Txt_C").unwrap();
    let time_sel = Selector::parse("td.Time span.RaceTime").unwrap();
    let pop_sel = Selector::parse("td.Odds span.OddsPeople").unwrap();
    let odds_sel = Selector::parse("td.Odds span.Odds_Ninki").unwrap();
    let bg_time_sel = Selector::parse("td.Time").unwrap();

    let mut found_results = false;
    for tr in doc.select(&tr_sel) {
        let rank_txt = tr
            .select(&rank_sel)
            .next()
            .map(|el| clean_text(&el.text().collect::<String>()))
            .unwrap_or_default();
        let Ok(actual_rank) = rank_txt.parse::<u32>() else {
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
        found_results = true;

        let actual_time = tr
            .select(&time_sel)
            .next()
            .map(|el| clean_text(&el.text().collect::<String>()))
            .filter(|s| !s.is_empty());

        let pop = tr
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

        if let Some(entry) = entries.iter_mut().find(|e| e.umaban == umaban) {
            entry.actual_rank = Some(actual_rank);
            if actual_time.is_some() {
                entry.actual_time = actual_time;
            }
            if last_3f.is_some() {
                entry.actual_last_3f = last_3f;
            }
            if entry.win_odds.is_none() && win_odds.is_some() {
                entry.win_odds = win_odds;
            }
            if entry.popularity.is_none() && pop.is_some() {
                entry.popularity = pop;
            }
        }
    }

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

    (found_results, payouts)
}

pub async fn scrape_race_data(
    input: &str,
) -> Result<(RaceInfo, Vec<HorseEntry>, Vec<OfficialPayoutItem>), String> {
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
    if !is_nar {
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
        let (is_done, payouts) = parse_result_and_payouts(&res_html, &mut entries);
        if is_done {
            race_info.is_finished = true;
            official_payouts = payouts;
        }
    }

    race_info.head_count = entries.iter().filter(|e| !e.is_scratched).count() as u32;

    Ok((race_info, entries, official_payouts))
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

    let surface = if d01_txt.contains('芝') {
        "芝".to_string()
    } else if d01_txt.contains('ダ') {
        "ダ".to_string()
    } else if d01_txt.contains('障') {
        "障".to_string()
    } else {
        "芝".to_string()
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
