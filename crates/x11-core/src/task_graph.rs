use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus { Pending, Running, Succeeded, Failed, Blocked, Cancelled }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    #[serde(default)] pub dependencies: Vec<Uuid>,
    pub status: TaskStatus,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct TaskGraph { tasks: BTreeMap<Uuid, Task> }

impl TaskGraph {
    pub fn add(&mut self, title: impl Into<String>, description: impl Into<String>, dependencies: Vec<Uuid>) -> Result<Uuid,String> {
        if dependencies.iter().any(|dep| !self.tasks.contains_key(dep)) { return Err("task dependency does not exist".into()); }
        let id=Uuid::new_v4();
        self.tasks.insert(id,Task{id,title:title.into(),description:description.into(),dependencies,status:TaskStatus::Pending});
        Ok(id)
    }
    pub fn get(&self,id:&Uuid)->Option<&Task>{self.tasks.get(id)}
    pub fn update(&mut self,id:Uuid,status:TaskStatus)->bool{self.tasks.get_mut(&id).map(|t|{t.status=status;true}).unwrap_or(false)}
    pub fn runnable(&self)->Vec<&Task>{self.tasks.values().filter(|task|task.status==TaskStatus::Pending && task.dependencies.iter().all(|dep|self.tasks.get(dep).is_some_and(|d|d.status==TaskStatus::Succeeded))).collect()}
    pub fn topological(&self)->Result<Vec<Uuid>,String>{
        let mut indegree:BTreeMap<Uuid,usize>=self.tasks.iter().map(|(id,t)|(*id,t.dependencies.len())).collect();
        let mut reverse:BTreeMap<Uuid,Vec<Uuid>>=BTreeMap::new();
        for (id,t) in &self.tasks{for dep in &t.dependencies{reverse.entry(*dep).or_default().push(*id);}}
        let mut q=VecDeque::from(indegree.iter().filter_map(|(id,n)|(*n==0).then_some(*id)).collect::<Vec<_>>());
        let mut out=Vec::with_capacity(self.tasks.len());
        while let Some(id)=q.pop_front(){out.push(id);for next in reverse.get(&id).into_iter().flatten(){let n=indegree.get_mut(next).expect("reverse graph only contains registered tasks");*n-=1;if *n==0{q.push_back(*next);}}}
        if out.len()!=self.tasks.len(){return Err("task graph contains a dependency cycle".into())}Ok(out)
    }
    pub fn blocked(&self)->BTreeSet<Uuid>{self.tasks.values().filter(|t|t.status==TaskStatus::Pending && t.dependencies.iter().any(|d|self.tasks.get(d).is_some_and(|x|matches!(x.status,TaskStatus::Failed|TaskStatus::Blocked|TaskStatus::Cancelled)))).map(|t|t.id).collect()}
    pub fn tasks(&self)->impl Iterator<Item=&Task>{self.tasks.values()}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn graph_orders_and_detects_blocking() {
        let mut g=TaskGraph::default();
        let a=g.add("a","",vec![]).unwrap();
        let b=g.add("b","",vec![a]).unwrap();
        assert_eq!(g.topological().unwrap(),vec![a,b]);
        assert_eq!(g.runnable()[0].id,a);
        g.update(a,TaskStatus::Succeeded); assert_eq!(g.runnable()[0].id,b);
        g.update(b,TaskStatus::Failed); assert!(g.blocked().is_empty());
        let c=g.add("c","",vec![b]).unwrap(); assert!(g.blocked().contains(&c));
    }
    #[test]
    fn graph_rejects_missing_dependency() { let mut g=TaskGraph::default(); assert!(g.add("bad","",vec![Uuid::new_v4()]).is_err()); }
}
