#[derive(Debug)]
struct Student {
    name: String,
    score: Vec<u32>,
}
#[derive(Debug)]
enum Grade {
    Excellent,
    Good,
    Pass,
    Fail,
}
impl Student {
    fn averge(&self) -> f64 {
        let total_score: u32 = self.score.iter().sum();
        let subject_count = self.score.len() as f64;
        total_score as f64 / subject_count
    }
    fn grade(&self) -> Grade {
        let avg = self.averge();
        match avg {
            a if a >= 90.0 => Grade::Excellent,
            a if a >= 75.0 => Grade::Good,
            a if a >= 60.0 => Grade::Pass,
            _ => Grade::Fail,
        }
    }
}
fn main() {
    let stu1 = Student {
        name: String::from("张三"),
        score: vec![92, 88, 95],
    };
    let stu2 = Student {
        name: String::from("李四"),
        score: vec![80, 70, 60],
    };
    let stu3 = Student {
        name: String::from("王五"),
        score: vec![50, 55, 58],
    };
    let student_list = vec![stu1, stu2, stu3];
    for student in student_list {
        let avg = student.averge();
        let level = student.grade();
        println!(
            "姓名：{} | 平均分：{:.2} | 成绩等级：{:?}",
            student.name, avg, level
        );
    }
}
