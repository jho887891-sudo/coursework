/// 学生成绩管理系统

/// 学生结构体
struct Student {
    name: String,
    scores: Vec<u32>,
}

/// 成绩等级枚举
#[derive(Debug, PartialEq)]
enum Grade {
    Excellent,
    Good,
    Pass,
    Fail,
}

impl std::fmt::Display for Grade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Grade::Excellent => write!(f, "优秀"),
            Grade::Good     => write!(f, "良好"),
            Grade::Pass     => write!(f, "及格"),
            Grade::Fail     => write!(f, "不及格"),
        }
    }
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
    // 创建 3 个学生
    let students = vec![
        Student {
            name: String::from("张三"),
            scores: vec![95, 88, 92, 90, 87],
        },
        Student {
            name: String::from("李四"),
            scores: vec![78, 82, 75, 80, 73],
        },
        Student {
            name: String::from("王五"),
            scores: vec![55, 60, 48, 52, 58],
        },
    ];

    println!("===== 学生成绩管理系统 =====");
    println!();

    for student in &students {
        println!(
            "姓名: {:>4}  |  平均分: {:>6.2}  |  等级: {}",
            student.name,
            student.average(),
            student.grade(),
        );
    }

    println!();
    println!("============================");
}
