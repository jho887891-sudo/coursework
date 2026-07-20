#[derive(Debug)]
struct Student{
    name: String,
    scores: Vec<u32>,
}

enum Grade {
    Excellent,
    Good,
    Pass,
    Fail,
}

impl std::fmt::Display for Grade{
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{
        match self {
            Grade::Excellent=>write!(f,"Excellent"),
            Grade::Good=>write!(f,"Good"),
            Grade::Pass=>write!(f,"Pass"),
            Grade::Fail=>write!(f,"Fail"),
        }
    }
}
impl Student{
    fn average(&self)->f64{
        let total:u32=self.scores.iter().sum();
        total as f64/self.scores.len() as f64
    }

    fn grade(&self)->Grade{
        let avg=self.average();
        if avg>=90.0{
            Grade::Excellent
        }else if avg>=75.0{
            Grade::Good
        }else if avg>=60.0{
            Grade::Pass
        }else{
            Grade::Fail
        }
    }
}



pub fn main(){
    let s1=Student{
        name:"张三".to_string(),
        scores:vec![90,95,92],
    };
    let s2=Student{
        name:"李四".to_string(),
        scores:vec![80,76,78],
    };
    let s3=Student{
        name:"王五".to_string(),
        scores:vec![60,65,70],
    };

    for student in [&s1,&s2,&s3]{
        let avg=student.average();
        let g=student.grade();
        println!(
            "姓名：{} | 平均分：{:.2} | 等级：{}",
            student.name,avg,g
        );
    }
}
