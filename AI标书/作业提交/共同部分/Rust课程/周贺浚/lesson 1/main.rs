
// 定义一个结构体
struct Student {
    name:String,
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
        let sum:u32=self.scores.iter().sum();
        sum as f64/self.scores.len() as f64
    }
    fn grade(&self) -> Grade {
        let avg:f64=self.average();
        if avg>=90.0 {
            return Grade::Excellent;
        } else if avg>=75.0 {
            return Grade::Good;
        } else if avg>=60.0 {
            return Grade::Pass;
        } else {
            return Grade::Fail;
        }
        
    }
}



fn main() {
    let xiaoming=Student { name:"小明".into(),scores:vec![90,92]};
    let xiaohong: Student=Student { name: "小红".into(), scores: vec![81,95] };
    let xiaogang: Student=Student { name:"小刚".into(),scores:vec![72,70]};
    println!("{}: average:{} grade:{:?}",xiaoming.name, xiaoming.average(),xiaoming.grade());
    println!("{}: average:{} grade:{:?}",xiaohong.name, xiaohong.average(),xiaohong.grade());
    println!("{}: average:{} grade:{:?}",xiaogang.name,xiaogang.average(), xiaogang.grade());
}
