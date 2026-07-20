//成绩等级枚举
#[derive(Debug)]
enum Grade {
    Excellent,
    Good,
    Pass,
    Fail,
}

//学生结构体
struct Student{
    name:String,
    scores:Vec<u32>,
}

//Student结构体的实现方法
impl Student{
    //计算平均分
    fn average(&self) -> f64 {
        let mut sum = 0;
        for score in &self.scores {
            sum += score; 
        }
        sum as f64 / self.scores.len() as f64
    }
    //返回等级
    fn grade(&self) -> Grade{
        let avg = self.average();
        if avg >= 90.0 {
            Grade::Excellent
        }else if avg >=75.0 {
            Grade::Good
        }else if avg >= 60.0 {
            Grade::Pass
        }else{
            Grade::Fail
        }
    }
}

fn main(){
    let s1 = Student {
        name: String::from("小明"),
        scores:vec![99,58,66]
    };
    let s2 = Student {
        name: String::from("小刚"),
        scores:vec![99,100,68]
    };
    let s3 = Student {
        name: String::from("小红"),
        scores:vec![94,88,100]
    };

    let stu_vec = vec![s1,s2,s3];

    for stu in &stu_vec {
        println!("姓名：{}，平均分：{:.2}，等级：{:?}",stu.name,stu.average(),stu.grade());
    }
}
