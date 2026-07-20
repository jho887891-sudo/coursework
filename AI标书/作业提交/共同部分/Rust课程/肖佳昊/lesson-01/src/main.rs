#[derive(Debug)]
enum Subject {
    Math,
    English,
    Science,
    History,
}

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
    scores: Vec<(Subject, u32)>,
}

impl Student {
    fn average(&self) -> f64 {
        if self.scores.is_empty() {
            0.0
        } else {
            let sum: u32 = self.scores.iter().map(|(_, score)| score).sum();
            sum as f64 / self.scores.len() as f64
        }
    }

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
            name: String::from("小明"),
            scores: vec![
                (Subject::Math, 95),
                (Subject::English, 88),
                (Subject::Science, 92),
                (Subject::History, 85),
            ],
        },
        Student {
            name: String::from("小红"),
            scores: vec![
                (Subject::Math, 78),
                (Subject::English, 82),
                (Subject::Science, 75),
                (Subject::History, 80),
            ],
        },
        Student {
            name: String::from("小刚"),
            scores: vec![
                (Subject::Math, 55),
                (Subject::English, 62),
                (Subject::Science, 58),
                (Subject::History, 49),
            ],
        },
    ];

    for student in students {
        println!(
            "姓名: {}, 平均分: {:.2}, 等级: {:?}",
            student.name,
            student.average(),
            student.grade()
        );
    }

    let s1 = String::from("hello");
    let s2 = s1.clone();
    println!("s1: {}, s2: {}", s1, s2);
}
