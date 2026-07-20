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

// #[derive(Debug)]	仅能打印调试
// #[derive(Debug, PartialEq)]	能打印 + 能用 ==/!= 比较

//impl就是给结构体加方法，struct+impl就是完整的对象
impl Student {
    fn average(&self) -> f64 {
        if self.scores.is_empty() {
            return 0.0;
        }

        let total: u32 = self.scores.iter().sum();
        total as f64 / self.scores.len() as f64
//         Rust 函数返回值规则
//         规则 1：最后一行表达式，不加 ;，就是自动返回值
//         规则 2：加了 ; 就变成语句，不返回任何东西
    }

    fn grade(&self) -> Grade {
        match self.average() {
            score if score >= 90.0 => Grade::Excellent,
            score if score >= 75.0 => Grade::Good,
            score if score >= 60.0 => Grade::Pass,
            _ => Grade::Fail,
        }
    }
    //     等价于：
    // fn grade(&self) -> Grade{
    //     let avg = self.average();
    //     if avg >= 90.0 {
    //         Grade::Excellent   // 返回 优秀
    //     } else if avg >=75.0 {
    //         Grade::Good        // 返回 良好
    //     } else if avg >= 60.0 {
    //         Grade::Pass        // 返回 及格
    //     }else{
    //         Grade::Fail        // 返回 不及格
    //     }
    // }
}

fn main() {
    let students = vec![
        Student {
            name: "小林".into(),
            scores: vec![95, 91, 94],
        },
        Student {
            name: "小周".into(),
            scores: vec![82, 76, 79],
        },
        Student {
            name: "小陈".into(),
            scores: vec![61, 58, 63],
        },
    ];

    // &students 是借用，因此循环结束后 students 仍然可用。
    for student in &students {
        println!(
            "{}：平均分 {:.1}，等级 {:?}",
            student.name,
            student.average(),
            student.grade()
        );
    }
    println!("共 {} 名学生", students.len());
}

// for student in students 等价于 for student in students.into_iter()。
// into_iter() 在循环一开始就拿走 students 的所有权（移动），
// 循环结束后再访问 students.len()，编译器直接报错：value borrowed here after move。
//
// 移动的是什么？Vec 栈上的三个字段——指针、长度、容量——整体转移到迭代器内部。
//
// ── 原本的内存布局 ──
//
//   [students]             【堆内存】
//   ├─ ptr ──────────────→ [s1, s2, s3]
//   ├─ len = 3
//   └─ cap = 3
//
// ── students.into_iter() 之后：students 已被移动，迭代器接管数据 ──
//
//        [students]              【迭代器】               【堆内存】
//        (已无效，为空)           ├─ ptr  → ───────────→ [s1, s2, s3]
//                                ├─ len = 3
//                                └─ cap = 3
// ── 对比：&students 等价于 students.iter()，只借用，不拿走所有权 ──
//
//        [students]             【迭代器】             【堆内存】
//        ├─ ptr → ───────────→  *ptr ───────────────→ [s1,s2,s3]
//        ├─ len=3                │
//        └─ cap=3                └─ 只是借用指针，不拿走所有权
//   students 仍然有效，循环后可继续访问
//   ── 对比：&mut students 等价于 students.iter_mut()，借用可变引用 ──
//        [students]              【&mut 迭代器】          【堆内存】
//        ├─ ptr  → ───────────→ ──┼─ *mut ptr ─────────→ [s1, s2, s3]
//        ├─ len = 3                │  可以修改!!！
//        └─ cap = 3                │
//                            【独占！唯一！】

#[cfg(test)] //这一段只有运行cargo test时才会编译，正常运行不执行
mod tests {
    use super::*;

    #[test]
    fn empty_scores_are_fail_instead_of_dividing_by_zero() {
        let student = Student {
            name: "缺考学生".into(),
            scores: vec![],
        };

        assert_eq!(student.average(), 0.0);
        assert_eq!(student.grade(), Grade::Fail);
    }
}
