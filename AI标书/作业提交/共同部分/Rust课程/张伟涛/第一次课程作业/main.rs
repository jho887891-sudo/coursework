struct Student {
    name : String,
    scores : Vec<u32>
}

enum Grade {
    Excellent,
    Good,
    Pass,
    Fail
}

impl Student {
    /// 计算平均分
    fn average(&self) -> f64 {
        if self.scores.is_empty() {
            return 0.0;
        }
        let sum: u32 = self.scores.iter().sum();
        sum as f64 / self.scores.len() as f64
    }

    /// 根据平均分返回等级
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
    let students = vec![
        Student {
            name: String::from("张三"),
            scores: vec![95, 88, 92],
        },
        Student {
            name: String::from("李四"),
            scores: vec![72, 78, 80],
        },
        Student {
            name: String::from("王五"),
            scores: vec![55, 60, 48],
        },
    ];

    for student in &students {
        let grade_str = match student.grade() {
            Grade::Excellent => "优秀",
            Grade::Good => "良好",
            Grade::Pass => "及格",
            Grade::Fail => "不及格",
        };
        println!(
            "姓名: {}, 平均分: {:.2}, 等级: {}",
            student.name,
            student.average(),
            grade_str
        );
    }
}
