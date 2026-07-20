/// 学生成绩管理系统

#[derive(Debug)]
struct Student {
    name: String,
    scores: Vec<u32>,
}

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
    fn new(name: &str, scores: Vec<u32>) -> Self {
        Student {
            name: name.into(),
            scores,
        }
    }

    fn average(&self) -> f64 {
        if self.scores.is_empty() {
            return 0.0;
        }
        let total: u32 = self.scores.iter().sum();
        total as f64 / self.scores.len() as f64
    }

    fn grade(&self) -> Grade {
        match self.average() {
            avg if avg >= 90.0 => Grade::Excellent,
            avg if avg >= 75.0 => Grade::Good,
            avg if avg >= 60.0 => Grade::Pass,
            _                  => Grade::Fail,
        }
    }

    fn highest(&self) -> Option<u32> {
        self.scores.iter().max().copied()
    }

    fn lowest(&self) -> Option<u32> {
        self.scores.iter().min().copied()
    }
}

fn main() {
    let students = vec![
        Student::new("张三", vec![95, 88, 92, 90, 87]),
        Student::new("李四", vec![78, 82, 75, 80, 73]),
        Student::new("王五", vec![55, 60, 48, 52, 58]),
    ];

    println!("{:-^40}", " 学生成绩管理系统 ");
    println!("{:<6} {:>6} {:>6} {:>6}  {}", "姓名", "平均分", "最高", "最低", "等级");
    println!("{}", "-".repeat(40));

    for s in &students {
        println!(
            "{:<6} {:>6.1} {:>6} {:>6}  {}",
            s.name,
            s.average(),
            s.highest().unwrap_or(0),
            s.lowest().unwrap_or(0),
            s.grade(),
        );
    }

    println!("{}", "-".repeat(40));
    println!("共 {} 名学生", students.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn average_is_correct() {
        let s = Student::new("测试", vec![80, 90, 100]);
        assert!((s.average() - 90.0).abs() < f64::EPSILON);
    }

    #[test]
    fn empty_scores_returns_zero() {
        let s = Student::new("缺考", vec![]);
        assert_eq!(s.average(), 0.0);
        assert_eq!(s.grade(), Grade::Fail);
    }

    #[test]
    fn grade_boundaries() {
        let excellent = Student::new("A", vec![95, 95]);
        let good      = Student::new("B", vec![80, 80]);
        let pass      = Student::new("C", vec![65, 65]);
        let fail      = Student::new("D", vec![50, 50]);

        assert_eq!(excellent.grade(), Grade::Excellent);
        assert_eq!(good.grade(),      Grade::Good);
        assert_eq!(pass.grade(),      Grade::Pass);
        assert_eq!(fail.grade(),      Grade::Fail);
    }

    #[test]
    fn grade_boundary_exact() {
        // 边界值测试
        let s90 = Student::new("90", vec![90]);
        let s75 = Student::new("75", vec![75]);
        let s60 = Student::new("60", vec![60]);
        let s59 = Student::new("59", vec![59]);

        assert_eq!(s90.grade(), Grade::Excellent);
        assert_eq!(s75.grade(), Grade::Good);
        assert_eq!(s60.grade(), Grade::Pass);
        assert_eq!(s59.grade(), Grade::Fail);
    }
}
