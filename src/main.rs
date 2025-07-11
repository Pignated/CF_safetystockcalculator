#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use egui_extras::{Column, TableBuilder};
use odbc_api::{ConnectionOptions, Cursor, Environment, ResultSetMetadata};
use statrs::distribution::ContinuousCDF;
use std::error::Error;

pub struct DdltApp {
    name: String,
    years: usize,
    table:Vec<Vec<String>>
}
impl Default for DdltApp {
    fn default() -> Self {
        Self {
            name: "".to_owned(),
            years: 3,
            table: Vec::new()
        }
    }
}
impl eframe::App for DdltApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_pixels_per_point(1.5);
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Safety Stock Calculator");
            let text_edit_resp = ui.text_edit_singleline(&mut self.name);
            if text_edit_resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                // If the user pressed Enter, trigger the search
                self.table =
                    calculate_ddlt(self.name.to_owned(), self.years).expect("Failed to calculate DDlt");
            }
            ui.add_space(10.0);
            if ui.button("Search Item").clicked() {
                self.table =
                    calculate_ddlt(self.name.to_owned(), self.years).expect("Failed to calculate DDlt");
            }
            ui.add(egui::Slider::new(&mut self.years, std::ops::RangeInclusive::<usize>::new(1,15)).text("Years"));
            ui.separator();
            if !self.table.is_empty() {
                //Create a widget to display the table, on each click update the widget with new table
                TableBuilder::new(ui)
                    .columns(
                        Column::auto().at_least(50.0).resizable(true),
                        self.table[0].len(),
                    )
                    .striped(true)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                    .header(20.0, |mut header| {
                        for col in &self.table[0] {
                            header.col(|ui| {
                                ui.label(col);
                            });
                        }
                    })
                    .body(|mut body| {
                        for row in self.table.iter().skip(1) {
                            body.row(20.0, |mut row_ui| {
                                for cell in row {
                                    row_ui.col(|ui| {
                                        ui.label(cell);
                                    });
                                }
                            });
                        }
                    });
            }
        });
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    {
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_icon(std::sync::Arc::new(egui::IconData {
                    rgba: image::load_from_memory(include_bytes!("../assets/icon.png"))
                        .unwrap()
                        .to_rgba8()
                        .to_vec(),
                    width: 48,
                    height: 48,
                }))
                .with_inner_size([1600.0, 300.0]),
            ..Default::default()
        };

        eframe::run_native("Safety Stock Calculator", options, Box::new(|_| Ok(Box::<DdltApp>::default())))
            .expect("Failed to run the egui application");
    }

    Ok(())
}

fn calculate_ddlt(item_name: String, years: usize) -> Result<Vec<Vec<String>>, Box<dyn Error>> {
    let env = Environment::new().expect("Failed to create ODBC environment");
    let connection_string = env!("CONNECTION_STRING");
    let conn = env
        .connect_with_connection_string(connection_string, ConnectionOptions::default())
        .expect("Failed to connect to the database");
    let lead_time_str = format!("select lead_time from item_mst where item = '{item_name}'");
    let lead_time: i32 = match conn.execute(&lead_time_str, (), None) {
        Ok(Some(mut cursor)) => {
            if let Some(mut row) = cursor.next_row().unwrap() {
                let mut buf = Vec::new();
                row.get_text(1, &mut buf).unwrap();
                String::from_utf8(buf).unwrap().parse().unwrap()
            } else {
                0 // Default lead time if not found
            }
        }
        _ => 0, // Default lead time if query fails
    };
    let query_string = format!(
        r#"
        declare @item_name nvarchar(50) = '{item_name}';

            with
                calendar
                as
                (
                                select dateadd(year, -{years}, cast(getdate() as date)) as calendar_date
                    union all
                        select dateadd(day, 1, calendar_date)
                        from calendar
                        where calendar_date < cast(getdate() as date)
                ),
                jobwhsedates as (
                    select j.whse, cast(j.job_date as date) as job_date, j.job
                    from job_mst j
                        join jobmatl_mst jm on j.job = jm.job
                    where jm.item = @item_name and j.job_date >= dateadd(year, -{years}, cast(getdate() as date))
                ),
                whses
                as
                (
                                select distinct whse
                        from coitem_mst
                        where item = @item_name
                    UNION
                        select distinct whse
                        from jobwhsedates
                )

                ,
                whses_date
                as
                (
                    select calendar.calendar_date, whses.whse
                    from calendar
                        cross join whses
                    where calendar.calendar_date <= cast(getdate() as date)
                ),
                item_info
                as
                (
                    select item, description, lead_time, plan_code, order_max, order_min, order_mult                    from item_mst
                    where item = @item_name
                ),
                item_whse
                as
                (
                    select whse, item_info.item, description, lead_time, qty_reorder, plan_code, order_max, order_min, order_mult
                    from itemwhse_mst
                        join item_info on itemwhse_mst.item = item_info.item
                    where itemwhse_mst.item = @item_name
                ),
                usages
                as
                (
                    select
                        whses_date.whse,
                        whses_date.calendar_date,
                        isnull(sum(combo.daily_usage),0) as daily_usage,
                        isnull(sum(combo.hits),0)
as hits
                    from whses_date
                        left join (

                                          select coitem_mst.whse as whse, coitem_mst.item as item, sum(coitem_mst.qty_shipped) as daily_usage, cast(coitem_mst.ship_date as date) as cal_date, count(*) as hits
                            from coitem_mst
                            where coitem_mst.item = @item_name and coitem_mst.ship_date >= dateadd(year, -{years}, cast(getdate() as date))
                            group by coitem_mst.whse, coitem_mst.item, cast(coitem_mst.ship_date as date)
                        union
                            select j.whse, jm.item, SUM(jm.matl_qty) as daily_usage, cast(j.job_date as date) as cal_date, count(*) as hits
                            from jobmatl_mst jm
                                join jobwhsedates j on jm.job = j.job
                            where j.job_date is not null and jm.item = @item_name and
j.job_date >= dateadd(year, -{years}, cast(getdate() as date))
                            group by j.whse, jm.item, cast(j.job_date as date)
                        ) as combo
                        on combo.whse = whses_date.whse and combo.cal_date = whses_date.calendar_date
                    group by whses_date.whse, whses_date.calendar_date
                ),
                ddlt_table
                as
                (

                    select whse, calendar_date, sum(daily_usage) over (
                            PARTITION BY whse
                            ORDER BY calendar_date
                            ROWS BETWEEN current row and {lead_time} following
                        ) as usage_sums
                    from usages

                )

            select ddlt_table.whse as "Warehouse", plan_code as "Planner Code", item, description, lead_time as "Lead time", order_max as "Order Max", order_min as
"Order Min", order_mult as "Order Multiple", qty_reorder as "Safety Stock", avg(usage_sums) as "Average DDLT", stdev(usage_sums) as "STDEV DDLT", max(usage_sums) as "MAX DDLT", min(usage_sums) as "MIN DDLT"
            from ddlt_table
                join item_whse on ddlt_table.whse = item_whse.whse
            where item_whse.item = @item_name
            GROUP by ddlt_table.whse, item, description, lead_time, qty_reorder, plan_code, order_max, order_min, order_mult
            option
            (maxrecursion
            0);
    "#

    );
    println!("{}",query_string);
    //TODO Change into use row buffer so you can check if there are more rows without consuming them
    match conn.execute(&query_string, (), None) {
        Ok(Some(mut cursor)) => {
            println!("awooooga");
            let colnames: Vec<String> = cursor.column_names()?.collect::<Result<_, _>>()?;
            let mut table: Vec<Vec<String>> = Vec::new();
            let mut columns: Vec<String> = Vec::new();
            for col in colnames.iter() {
                columns.push(col.to_string());
            }
            table.push(columns.clone());
            while let Some(mut row) = cursor.next_row()? {
                let mut row_data: Vec<String> = Vec::new();
                for i in 1..colnames.len() + 1 {
                    let mut buf = Vec::new();
                    row.get_text(i as u16, &mut buf)?;
                    let value = String::from_utf8_lossy(&buf).to_string();
                    if is_numeric(value.clone().as_str()) {
                        let num_value: f64 = value.parse().unwrap_or(0.0);
                        row_data.push(format!("{:.2}", num_value));
                    } else {
                        row_data.push(value.clone());
                    }
                }
                let st_dev = row_data[10].parse::<f64>().unwrap_or(0.0);
                row_data.push(format!(
                    "{:.2}",
                    st_dev
                        * statrs::distribution::Normal::new(0.0, 1.0)
                            .expect("bwah")
                            .inverse_cdf(0.95)
                ));
                row_data.push(format!(
                    "{:.2}",
                    st_dev
                        * statrs::distribution::Normal::new(0.0, 1.0)
                            .expect("bwah")
                            .inverse_cdf(0.97)
                ));
                row_data.push(format!(
                    "{:.2}",
                    st_dev
                        * statrs::distribution::Normal::new(0.0, 1.0)
                            .expect("bwah")
                            .inverse_cdf(0.98)
                ));

                row_data.push(format!(
                    "{:.2}",
                    st_dev
                        * statrs::distribution::Normal::new(0.0, 1.0)
                            .expect("bwah")
                            .inverse_cdf(0.99)
                ));
                row_data.push(format!(
                    "{:.2}",
                    st_dev
                        * statrs::distribution::Normal::new(0.0, 1.0)
                            .expect("bwah")
                            .inverse_cdf(0.999)
                ));

                table.push(row_data.clone());
            }
            table[0].push("95%".to_string());
            table[0].push("97%".to_string());
            table[0].push("98%".to_string());
            table[0].push("99%".to_string());
            table[0].push("99.9%".to_string());
            Ok(table)
        }
        Err(e) => {
            println!("{}",e);
            println!("{}",query_string);
            println!("No results returned from the query or an error occurred.");
            Ok(Vec::new())
        }
        Ok(_) => {
            println!("No results returned from the query.");
            Ok(Vec::new())
        }
    }
}
fn is_numeric(s: &str) -> bool {
    s.parse::<f64>().is_ok()
}


