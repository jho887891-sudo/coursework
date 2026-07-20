#[derive(Debug)]
enum Grade {
    Excellent,
    Good,
    Pass,
    Fail,
}

#[derive(Debug)]
struct Student {
    name: String,
    scores: Vec<u32>,
}

impl Student {
    // 计算平均分。
    fn average(&self) -> f64 {
        if self.scores.is_empty() {
            return 0.0;
        }
        let mut sum = 0;
        for score in &self.scores {
            sum += score;
        }
        //做类型强转
        sum as f64 / self.scores.len() as f64
    }

    // 根据平均分算出等级
    fn grade(&self) -> Grade {
        let avg = self.average();
        if avg >= 90.0 {
            Grade::Excellent
        } else if avg >= 75.0 {
            Grade::Good
        } else if avg >= 60.0 {
            Grade::Pass
        } else {
            Grade::Fail
        }
    }
}

fn main() {
    // 用 .into() 把 &str 变成 String
    let s1 = Student {
        name: "小明".into(),
        scores: vec![90, 95, 88],
    };
    let s2 = Student {
        name: "小红".into(),
        scores: vec![78, 82, 75],
    };
    let s3 = Student {
        name: "小刚".into(),
        scores: vec![50, 60, 55],
    };

    let students = vec![s1, s2, s3];

    for s in &students {
        println!("姓名: {}, 平均分: {:.1}, 等级: {:?}", s.name, s.average(), s.grade());
    }
}