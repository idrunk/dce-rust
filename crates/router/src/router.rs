use std::any::type_name;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Debug;
use crate::request::RawRequest;
use crate::api::{ApiTrait, BeforeController};
use dce_util::mixed::{DceErr, DceResult};
use dce_util::string::merge_consecutive_char;
use dce_util::atom_tree::ATree;
use dce_util::atom_tree::{KeyFactory, TreeTraverResult};
use std::sync::Arc;
use log::debug;
use crate::request::{PathParam, RequestContext};

const PATH_PART_SEPARATOR: char = '/';
const VARIABLE_OPENER: char = '{';
const VARIABLE_CLOSING: char = '}';
const VAR_TYPE_OPTIONAL: char = '?';
const VAR_TYPE_EMPTABLE_VECTOR: char = '*';
const VAR_TYPE_VECTOR: char = '+';

pub const CODE_NOT_FOUND: isize = 404;

#[derive(Debug)]
pub struct Router<Raw: RawRequest + 'static> {
    apis: Vec<(String, &'static (dyn ApiTrait<Raw> + Send + Sync))>,
    omitted_paths: HashSet<String>,
    api_mapping: HashMap<&'static str, Vec<&'static (dyn ApiTrait<Raw> + Send + Sync)>>,
    api_trunk_tree: Arc<ATree<ApiTrunk<Raw>, &'static str>>,
    before_controller: Option<BeforeController<Raw>>,
}

impl<Raw: RawRequest + Debug + 'static> Router<Raw> {
    pub fn new() -> Self {
        Self {
            apis: vec![],
            omitted_paths: Default::default(),
            api_mapping: Default::default(),
            api_trunk_tree: ATree::new(ApiTrunk::new("", vec![])),
            before_controller: None,
        }
    }

    pub fn get_tree(&self) -> &Arc<ATree<ApiTrunk<Raw>, &'static str>> {
        &self.api_trunk_tree
    }

    pub fn get_before_controller(&self) -> &Option<BeforeController<Raw>> {
        &self.before_controller
    }

    pub fn before_controller(mut self, before_controller: BeforeController<Raw>) -> Self {
        self.before_controller = Some(before_controller);
        self
    }

    pub fn push(mut self, supplier: fn() -> &'static (dyn ApiTrait<Raw> + Send + Sync)) -> Self {
        let api = supplier();
        let formatted = merge_consecutive_char(api.path().trim_matches(PATH_PART_SEPARATOR), PATH_PART_SEPARATOR); // merge format slashes
        if api.omission() {
            self.omitted_paths.insert(formatted.clone());
        }
        self.apis.push((formatted, api));
        self
    }

    pub fn consumer_push(self, consumer: fn(Self) -> Self) -> Self {
        consumer(self)
    }

    fn omitted_path(&self, formatted: &String) -> String {
        let parts = formatted.split(PATH_PART_SEPARATOR).collect::<Vec<_>>();
        parts.iter().enumerate().filter(|(i, _)| !self.omitted_paths.contains(parts[0..=*i].join(PATH_PART_SEPARATOR.to_string().as_str()).as_str()))
            .map(|t| t.1.to_string()).collect::<Vec<_>>().join(PATH_PART_SEPARATOR.to_string().as_str())
    }

    fn closing(&mut self) {
        while let Some((path, api)) = self.apis.pop() {
            let path = self.omitted_path(&path);
            let mut apis = vec![api];
            for index in (0..self.apis.len()).rev() {
                if path.eq_ignore_ascii_case(self.omitted_path(&self.apis[index].0).as_str()) {
                    apis.insert(0, self.apis.remove(index).1);
                }
            }
            let path = Box::leak(path.into_boxed_str());
            self.api_mapping.insert(path, apis);
        }
        self.build_tree();
    }

    fn build_tree(&mut self) {
        // build NodeBox tree
        self.api_trunk_tree.build(
            self.api_mapping.iter().map(|(path, apis)| ApiTrunk::new(path, apis.clone())).collect::<Vec<_>>(),
            // if there had some remains api
            // fill parent NodeBoxes, for example only configured one api with path ["home/common/index"],
            // then it will be filled to ["home", "home/common", "home/common/index"]
            Some(|tree, mut remains| {
                let mut fills: BTreeMap<Vec<&'static str>, ApiTrunk<Raw>> = BTreeMap::new();
                while let Some(element) = remains.pop() {
                    let paths: Vec<_> = element.path.split(PATH_PART_SEPARATOR).collect();
                    for i in 0..paths.len() - 1 {
                        let path = paths[..=i].to_vec();
                        if matches!(tree.get_by_path(&path), None) && ! fills.contains_key(&path) {
                            let api_path = ApiTrunk::new(Box::leak(path.clone().join(PATH_PART_SEPARATOR.to_string().as_str()).into_boxed_str()), vec![]);
                            fills.insert(path, api_path);
                        }
                    }
                    // the remains element self should directly insert into
                    fills.insert(paths, element);
                }
                while let Some((paths, nb)) = fills.pop_first() {
                    tree.set_by_path(paths, nb).unwrap();
                }
            }),
        );
        self.api_trunk_tree.traversal(|tree| {
            if let Some(parent) = tree.parent() {
                let mut parent = parent.write().unwrap();
                match parent.var_type {
                    VarType::Required(_) => parent.is_mid_var = true,
                    VarType::NotVar => {},
                    _ => panic!("ambiguous type var '{}' cannot in middle.", parent.key()),
                }
                if ! matches!(tree.read().unwrap().var_type, VarType::NotVar) {
                    parent.var_children.push(tree.clone());
                }
            }
            TreeTraverResult::KeepOn
        });
    }

    pub fn ready(mut self) -> &'static mut Arc<Router<Raw>> {
        self.closing();
        Box::leak(Box::new(Arc::new(self)))
    }

    fn locate(
        &self,
        mut path: &str,
        api_finder: impl Fn(&Vec<&'static (dyn ApiTrait<Raw> + Send + Sync)>) -> DceResult<&'static (dyn ApiTrait<Raw> + Send + Sync)>,
    ) -> DceResult<(&'static (dyn ApiTrait<Raw> + Send + Sync), HashMap<&'static str, PathParam>)> {
        let mut api;
        let mut ab;
        let mut path_args = Default::default();
        // match api follow redirect
        loop {
            if let Some((tmp_path, tmp_path_args)) = if self.api_mapping.contains_key(path) { None } else { self.match_var_path(path) } {
                (path, path_args) = (tmp_path, tmp_path_args);
            }
            ab = self.api_mapping.get(path).ok_or(DceErr::openly(CODE_NOT_FOUND, format!(r#"path "{}" route failed, could not match in router"#, path)))?;
            api = api_finder(ab)?;
            if api.redirect().is_empty() {
                break;
            }
            path = api.redirect();
        }
        Ok((api, path_args))
    }

    fn match_var_path(
        &self,
        path: &str,
    ) -> Option<(&'static str, HashMap<&'static str, PathParam>)> {
        let path_parts = path.split(PATH_PART_SEPARATOR).collect::<Vec<_>>();
        let mut loop_items = vec![(self.api_trunk_tree.clone(), 0_usize)];
        let mut target_api_trunk = None;
        let mut path_args = HashMap::new();
        'outer: while let Some((api_trunk, part_number)) = loop_items.pop() {
            let overflowed = part_number >= path_parts.len();
            if overflowed && ! api_trunk.read().unwrap().apis.is_empty() {
                // should be finished at last request path part if not a bare tree
                target_api_trunk = Some(api_trunk.clone());
                break;
            }
            let children = api_trunk.children().read().unwrap();
            // if not overflow and request path matched, then it must be a normal path
            if let Some(sub_api_trunk) = if overflowed { None } else { children.get(&path_parts[part_number]) } {
                // push it into loop queue to handle it next cycle
                loop_items.push((sub_api_trunk.clone(), 1 + part_number));
            } else {
                let insert_pos = loop_items.len();
                for var_api_trunk in api_trunk.read().unwrap().var_children.clone() {
                    let var_api_trunk_read = var_api_trunk.read().unwrap();
                    if ! var_api_trunk_read.is_mid_var {
                        // if not a middle var, then should finish var path match and collect vars and end the outer loop
                        match var_api_trunk_read.var_type {
                            // should be a none optional parameter if it's overflowed
                            VarType::Optional(var_name) if overflowed => path_args.insert(var_name, PathParam::Opt(None)),
                            // should be a some optional parameter if it's not overflowed
                            VarType::Optional(var_name) if ! overflowed => path_args.insert(var_name, PathParam::Opt(Some(path_parts[part_number].to_string()))),
                            VarType::EmptableVector(var_name) if overflowed => path_args.insert(var_name, PathParam::Vec(vec![])),
                            VarType::EmptableVector(var_name) if ! overflowed => path_args.insert(var_name, PathParam::Vec(path_parts[part_number..].iter().map(|p| p.to_string()).collect::<Vec<_>>())),
                            // shouldn't be a valid vector if it's overflowed
                            VarType::Vector(var_name) if ! overflowed => path_args.insert(var_name, PathParam::Vec(path_parts[part_number..].iter().map(|p| p.to_string()).collect::<Vec<_>>())),
                            VarType::Required(var_name) if ! overflowed => path_args.insert(var_name, PathParam::Reqd(path_parts[part_number].to_string())),
                            // if it should be the end vars but overflowed, continue the for loop to let other var api path to match
                            _ => continue,
                        };
                        target_api_trunk = Some(var_api_trunk.clone());
                        break 'outer;
                    } else if let VarType::Required(var_name) = var_api_trunk_read.var_type {

                        // if it's middle var then insert to loop queue to handle it next cycle
                        path_args.insert(var_name, PathParam::Reqd(path_parts[part_number].to_string()));
                        loop_items.insert(insert_pos, (var_api_trunk.clone(), 1 + part_number));
                    }
                }
            }
        }
        target_api_trunk.map(|trunk| (trunk.read().unwrap().path, path_args))
    }

    #[cfg(feature = "async")]
    pub async fn route(context: RequestContext<Raw>) -> (Option<bool>, DceResult<Option<Raw::Resp>>) {
        match context.router().locate(context.raw().path(), |apis| Raw::api_match(context.raw(), apis)) {
            Ok((api, path_args)) => {
                debug!(r#"{}: path "{}" matched api "{}""#, type_name::<Raw>(), context.raw().path(), api.path());
                (Some(api.unresponsive()), api.call_controller(context.set_api(api).set_params(path_args)).await)
            },
            Err(err) => (None, Err(err)),
        }
    }

    #[cfg(not(feature = "async"))]
    pub fn route(context: RequestContext<Raw>) -> (Option<bool>, DceResult<Option<Raw::Resp>>) {
        match context.router().locate(context.raw().path(), |apis| Raw::api_match(context.raw(), apis)) {
            Ok((api, path_args)) => {
                debug!("{}: path '{}' matched api '{}'", type_name::<Raw>(), context.raw().path(), api.path());
                (Some(api.unresponsive()), api.call_controller(context.set_api(api).set_params(path_args)))
            },
            Err(err) => (None, Err(err)),
        }
    }
}

#[derive(PartialEq, Debug, Clone)]
enum VarType {
    Required(&'static str),
    Optional(&'static str),
    EmptableVector(&'static str),
    Vector(&'static str),
    NotVar,
}

#[derive(Debug)]
pub struct ApiTrunk<Raw: RawRequest + 'static> {
    path: &'static str,
    var_type: VarType,
    is_mid_var: bool,
    apis: Vec<&'static (dyn ApiTrait<Raw> + Send + Sync)>,
    var_children: Vec<Arc<ATree<ApiTrunk<Raw>, &'static str>>>,
}

impl<Raw: RawRequest> ApiTrunk<Raw> {
    fn new(path: &'static str, apis: Vec<&'static (dyn ApiTrait<Raw> + Send + Sync)>) -> ApiTrunk<Raw> {
        ApiTrunk {
            path,
            var_type: VarType::NotVar,
            is_mid_var: false,
            apis,
            var_children: vec![],
        }.fill_var_type()
    }

    fn fill_var_type(mut self) -> ApiTrunk<Raw> {
        let key = self.key();
        if key.starts_with(VARIABLE_OPENER) && key.ends_with(VARIABLE_CLOSING) {
            let var = key[1..key.len() - 1].trim();
            self.var_type = match var.chars().last() {
                Some(VAR_TYPE_OPTIONAL) => VarType::Optional(var[0..var.len() - 1].trim_end()),
                Some(VAR_TYPE_EMPTABLE_VECTOR) => VarType::EmptableVector(var[0..var.len() - 1].trim_end()),
                Some(VAR_TYPE_VECTOR) => VarType::Vector(var[0..var.len() - 1].trim_end()),
                _ => VarType::Required(var),
            }
        }
        self
    }
}

impl<Raw: RawRequest> KeyFactory<&'static str> for ApiTrunk<Raw> {
    fn key(&self) -> &'static str {
        if let Some(index) = self.path.rfind(PATH_PART_SEPARATOR) {
            return &self.path[index+1 ..].trim()
        }
        self.path.trim()
    }

    fn child_of(&self, parent: &Self) -> bool {
        if let Some(index) = self.path.rfind(PATH_PART_SEPARATOR) {

            &self.path[..index] == parent.path
        } else {
            parent.path.is_empty()
        }
    }
}

impl<Raw: RawRequest> PartialEq for ApiTrunk<Raw> {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}
