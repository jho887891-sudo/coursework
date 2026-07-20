#[derive(Debug)]
struct Student {
    name: String,
    scores: Vec<u32>,
}

#[derive(Debug)]
enum Grade {
    Excellent,
    Good,
    Pass,
    Fail,
}

impl Student {
   
    fn average(&self) -> f64 {

        let sum: u32 = self.scores.iter().sum();
        let count = self.scores.len() as f64;
        sum as f64 / count
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

    let s1 = Student{name:"小明".into(), scores:vec![92,95,90]};
    let s2 = Student{name:"小红".into(), scores:vec![78,72,81]};
    let s3 = Student{name:"小刚".into(), scores:vec![55,62,58]};

   
    fn print_student(s: &Student) {
        println!("姓名：{}，平均分：{:.2}，等级：{:?}", s.name, s.average(), s.grade());
    }

    print_student(&s1);
    print_student(&s2);
    print_student(&s3);
}