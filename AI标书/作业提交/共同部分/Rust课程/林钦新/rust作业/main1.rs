// ============================================================
// 学生成绩管理系统
// ============================================================

/// 成绩等级枚举
#[derive(Debug, PartialEq)]
enum Grade {
    Excellent, // ≥90
    Good,      // ≥75
    Pass,      // ≥60
    Fail,      // <60
}

/// 学生结构体
struct Student {
    name: String,
    scores: Vec<u32>,
}

impl Student {
    /// 计算平均分
    /// 如果没有成绩，返回 0.0
    fn average(&self) -> f64 {
        if self.scores.is_empty() {
            return 0.0;
        }
        // sum 返回 u32，用 as 转换为 f64
        let sum: u32 = self.scores.iter().sum();
        sum as f64 / self.scores.len() as f64
    }

    /// 根据平均分返回成绩等级
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
            scores: vec![95, 88, 92, 90],
        },
        Student {
            name: String::from("李四"),
            scores: vec![78, 82, 70, 75],
        },
        Student {
            name: String::from("王五"),
            scores: vec![55, 60, 48, 52],
        },
    ];

    // 打印每人的姓名、平均分和等级
    for student in &students {
        let avg = student.average();
        let grade = student.grade();
        // 将等级枚举转换为中文描述
        let grade_str = match grade {
            Grade::Excellent => "优秀",
            Grade::Good => "良好",
            Grade::Pass => "及格",
            Grade::Fail => "不及格",
        };
        println!(
            "姓名: {} | 平均分: {:.1} | 等级: {}",
            student.name, avg, grade_str
        );
    }
}
