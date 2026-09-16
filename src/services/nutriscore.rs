use serde_json::Value;

/// Standard Nutri-Score calculation based on Santé Publique France / EU Nutri-Score specifications.
/// Computes both the numerical Nutri-Score (-15 to +40) and the letter grade ("a"–"e").
/// Returns `None` if essential nutritional facts (energy, sugar, saturated fat, sodium) are missing.
pub fn calculate_nutriscore(nutrition_facts: &Value, is_beverage: bool) -> Option<(i32, String)> {
    let get_num = |key: &str| -> Option<f64> {
        nutrition_facts.get(key).and_then(|v| {
            if let Some(n) = v.as_f64() {
                Some(n)
            } else if let Some(s) = v.as_str() {
                s.trim().parse().ok()
            } else {
                None
            }
        })
    };

    let energy_kcal = get_num("energy_kcal")?;
    let sugar = get_num("sugar")?;
    let saturated_fat = get_num("saturated_fat")?;
    let sodium = get_num("sodium")?; // in grams per 100g

    let protein = get_num("protein").unwrap_or(0.0);
    let fiber = get_num("fiber").unwrap_or(0.0);

    let kj = energy_kcal * 4.184;
    let sodium_mg = sodium * 1000.0;

    let (score, grade) = if is_beverage {
        calculate_beverage_score(kj, sugar, saturated_fat, sodium_mg, protein, fiber)
    } else {
        calculate_solid_score(kj, sugar, saturated_fat, sodium_mg, protein, fiber)
    };

    Some((score, grade))
}

fn calculate_solid_score(
    kj: f64,
    sugar: f64,
    saturated_fat: f64,
    sodium_mg: f64,
    protein: f64,
    fiber: f64,
) -> (i32, String) {
    let energy_pts = if kj > 3350.0 { 10 }
    else if kj > 3015.0 { 9 }
    else if kj > 2680.0 { 8 }
    else if kj > 2345.0 { 7 }
    else if kj > 2010.0 { 6 }
    else if kj > 1675.0 { 5 }
    else if kj > 1340.0 { 4 }
    else if kj > 1005.0 { 3 }
    else if kj > 670.0 { 2 }
    else if kj > 335.0 { 1 }
    else { 0 };

    let sugar_pts = if sugar > 45.0 { 10 }
    else if sugar > 40.0 { 9 }
    else if sugar > 36.0 { 8 }
    else if sugar > 31.0 { 7 }
    else if sugar > 27.0 { 6 }
    else if sugar > 22.5 { 5 }
    else if sugar > 18.0 { 4 }
    else if sugar > 13.5 { 3 }
    else if sugar > 9.0 { 2 }
    else if sugar > 4.5 { 1 }
    else { 0 };

    let sat_fat_pts = if saturated_fat > 10.0 { 10 }
    else if saturated_fat > 9.0 { 9 }
    else if saturated_fat > 8.0 { 8 }
    else if saturated_fat > 7.0 { 7 }
    else if saturated_fat > 6.0 { 6 }
    else if saturated_fat > 5.0 { 5 }
    else if saturated_fat > 4.0 { 4 }
    else if saturated_fat > 3.0 { 3 }
    else if saturated_fat > 2.0 { 2 }
    else if saturated_fat > 1.0 { 1 }
    else { 0 };

    let sodium_pts = if sodium_mg > 900.0 { 10 }
    else if sodium_mg > 810.0 { 9 }
    else if sodium_mg > 720.0 { 8 }
    else if sodium_mg > 630.0 { 7 }
    else if sodium_mg > 540.0 { 6 }
    else if sodium_mg > 450.0 { 5 }
    else if sodium_mg > 360.0 { 4 }
    else if sodium_mg > 270.0 { 3 }
    else if sodium_mg > 180.0 { 2 }
    else if sodium_mg > 90.0 { 1 }
    else { 0 };

    let n_points = energy_pts + sugar_pts + sat_fat_pts + sodium_pts;

    let protein_pts = if protein > 8.0 { 5 }
    else if protein > 6.4 { 4 }
    else if protein > 4.8 { 3 }
    else if protein > 3.2 { 2 }
    else if protein > 1.6 { 1 }
    else { 0 };

    let fiber_pts = if fiber > 4.7 { 5 }
    else if fiber > 3.7 { 4 }
    else if fiber > 2.8 { 3 }
    else if fiber > 1.9 { 2 }
    else if fiber > 0.9 { 1 }
    else { 0 };

    // Nutri-Score rule: If N >= 11, protein is only subtracted if fruits/veg >= 5.
    let p_points = if n_points >= 11 {
        fiber_pts
    } else {
        protein_pts + fiber_pts
    };

    let total_score = n_points - p_points;

    let grade = match total_score {
        ..=-1 => "a",
        0..=2 => "b",
        3..=10 => "c",
        11..=18 => "d",
        _ => "e",
    };

    (total_score, grade.to_string())
}

fn calculate_beverage_score(
    kj: f64,
    sugar: f64,
    saturated_fat: f64,
    sodium_mg: f64,
    protein: f64,
    fiber: f64,
) -> (i32, String) {
    let energy_pts = if kj > 270.0 { 10 }
    else if kj > 240.0 { 9 }
    else if kj > 210.0 { 8 }
    else if kj > 180.0 { 7 }
    else if kj > 150.0 { 6 }
    else if kj > 120.0 { 5 }
    else if kj > 90.0 { 4 }
    else if kj > 60.0 { 3 }
    else if kj > 30.0 { 2 }
    else if kj > 0.0 { 1 }
    else { 0 };

    let sugar_pts = if sugar > 13.5 { 10 }
    else if sugar > 12.0 { 9 }
    else if sugar > 10.5 { 8 }
    else if sugar > 9.0 { 7 }
    else if sugar > 7.5 { 6 }
    else if sugar > 6.0 { 5 }
    else if sugar > 4.5 { 4 }
    else if sugar > 3.0 { 3 }
    else if sugar > 1.5 { 2 }
    else if sugar > 0.0 { 1 }
    else { 0 };

    let sat_fat_pts = if saturated_fat > 10.0 { 10 }
    else if saturated_fat > 9.0 { 9 }
    else if saturated_fat > 8.0 { 8 }
    else if saturated_fat > 7.0 { 7 }
    else if saturated_fat > 6.0 { 6 }
    else if saturated_fat > 5.0 { 5 }
    else if saturated_fat > 4.0 { 4 }
    else if saturated_fat > 3.0 { 3 }
    else if saturated_fat > 2.0 { 2 }
    else if saturated_fat > 1.0 { 1 }
    else { 0 };

    let sodium_pts = if sodium_mg > 900.0 { 10 }
    else if sodium_mg > 810.0 { 9 }
    else if sodium_mg > 720.0 { 8 }
    else if sodium_mg > 630.0 { 7 }
    else if sodium_mg > 540.0 { 6 }
    else if sodium_mg > 450.0 { 5 }
    else if sodium_mg > 360.0 { 4 }
    else if sodium_mg > 270.0 { 3 }
    else if sodium_mg > 180.0 { 2 }
    else if sodium_mg > 90.0 { 1 }
    else { 0 };

    let n_points = energy_pts + sugar_pts + sat_fat_pts + sodium_pts;

    let fiber_pts = if fiber > 4.7 { 5 }
    else if fiber > 3.7 { 4 }
    else if fiber > 2.8 { 3 }
    else if fiber > 1.9 { 2 }
    else if fiber > 0.9 { 1 }
    else { 0 };

    let protein_pts = if protein > 8.0 { 5 }
    else if protein > 6.4 { 4 }
    else if protein > 4.8 { 3 }
    else if protein > 3.2 { 2 }
    else if protein > 1.6 { 1 }
    else { 0 };

    let p_points = if n_points >= 11 {
        fiber_pts
    } else {
        protein_pts + fiber_pts
    };

    let total_score = n_points - p_points;

    let grade = match total_score {
        ..=1 => "b",
        2..=5 => "c",
        6..=9 => "d",
        _ => "e",
    };

    (total_score, grade.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn calculates_dairy_milk_chocolate_as_grade_e() {
        let facts = json!({
            "energy_kcal": 543.0,
            "sugar": 55.3,
            "saturated_fat": 21.5,
            "sodium": 0.125,
            "protein": 7.6,
            "fat": 31.4,
            "carbs": 58.5
        });

        let result = calculate_nutriscore(&facts, false);
        assert!(result.is_some());
        let (score, grade) = result.unwrap();
        assert_eq!(grade, "e");
        assert!(score >= 19);
    }

    #[test]
    fn calculates_healthy_oats_as_grade_a() {
        let facts = json!({
            "energy_kcal": 360.0,
            "sugar": 1.0,
            "saturated_fat": 1.1,
            "sodium": 0.005,
            "protein": 13.0,
            "fiber": 10.0,
            "fat": 6.0,
            "carbs": 60.0
        });

        let result = calculate_nutriscore(&facts, false);
        assert!(result.is_some());
        let (score, grade) = result.unwrap();
        assert_eq!(grade, "a");
        assert!(score <= -1);
    }

    #[test]
    fn calculates_sugary_soda_as_grade_e() {
        let facts = json!({
            "energy_kcal": 42.0,
            "sugar": 10.6,
            "saturated_fat": 0.0,
            "sodium": 0.01,
            "protein": 0.0
        });

        let result = calculate_nutriscore(&facts, true);
        assert!(result.is_some());
        let (_score, grade) = result.unwrap();
        assert_eq!(grade, "e");
    }

    #[test]
    fn returns_none_when_essential_facts_missing() {
        let facts = json!({
            "energy_kcal": 100.0
        });
        assert_eq!(calculate_nutriscore(&facts, false), None);
    }
}
