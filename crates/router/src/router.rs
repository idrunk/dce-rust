use std::any::type_name;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Debug;
use crate::api::{ApiTrait, EventHandler};
use dce_util::mixed::{DceErr, DceResult};
use dce_util::atom_tree::ATree;
use dce_util::atom_tree::{KeyFactory, TreeTraverBreak};
use std::sync::{Arc, RwLockReadGuard};
use log::debug;
use crate::protocol::RoutableProtocol;
use crate::request::{PathParam, Context};

pub const PATH_PART_SEPARATOR: char = '/';
pub const SUFFIX_BOUNDARY: char = '.';
const VARIABLE_OPENER: char = '{';
const VARIABLE_CLOSING: char = '}';
const VAR_TYPE_OPTIONAL: char = '?';
const VAR_TYPE_EMPTABLE_VECTOR: char = '*';
const VAR_TYPE_VECTOR: char = '+';

pub const CODE_NOT_FOUND: isize = 404;

#[derive(Debug)]
pub struct Router<Rp: RoutableProtocol + 'static> {
    path_part_separator: char,
    suffix_boundary: char,
    api_buffer: Vec<&'static (dyn ApiTrait<Rp> + Send + Sync)>,
    raw_omitted_paths: HashSet<&'static str>,
    id_api_mapping: HashMap<&'static str, &'static (dyn ApiTrait<Rp> + Send + Sync)>,
    // HashMap's key was the omitted path with suffix
    apis_mapping: HashMap<&'static str, Vec<&'static (dyn ApiTrait<Rp> + Send + Sync)>>,
    apis_tree: Arc<ATree<ApiBranch<Rp>, &'static str>>,
    before_controller: Option<EventHandler<Rp>>,
    after_controller: Option<EventHandler<Rp>>,
}

impl<Rp: RoutableProtocol + Debug + 'static> Router<Rp> {
    pub fn new() -> DceResult<Self> {
        Ok(Self {
            path_part_separator: PATH_PART_SEPARATOR,
            suffix_boundary: SUFFIX_BOUNDARY,
            api_buffer: vec![],
            raw_omitted_paths: Default::default(),
            id_api_mapping: Default::default(),
            apis_mapping: Default::default(),
            apis_tree: ATree::new(ApiBranch::new("", vec![]))?,
            before_controller: None,
            after_controller: None,
        })
    }

    pub fn set_separator(mut self, path_part_separator: char , suffix_separator: char) -> Self {
        self.path_part_separator = path_part_separator;
        self.suffix_boundary = suffix_separator;
        self
    }

    pub fn suffix_boundary(&self) -> char {
        self.suffix_boundary
    }

    pub fn apis_tree(&self) -> &Arc<ATree<ApiBranch<Rp>, &'static str>> {
        &self.apis_tree
    }

    pub fn before_controller(&self) -> &Option<EventHandler<Rp>> {
        &self.before_controller
    }

    pub fn after_controller(&self) -> &Option<EventHandler<Rp>> {
        &self.after_controller
    }

    pub fn set_event_handlers(mut self, before_controller: Option<EventHandler<Rp>>, after_controller: Option<EventHandler<Rp>>) -> Self {
        self.before_controller = before_controller;
        self.after_controller = after_controller;
        self
    }

    pub fn push(mut self, supplier: fn() -> &'static (dyn ApiTrait<Rp> + Send + Sync)) -> Self {
        let api = supplier();
        if api.omission() {
            self.raw_omitted_paths.insert(api.path());
        }
        if ! api.id().is_empty() {
            self.id_api_mapping.insert(api.id(), api);
        }
        self.api_buffer.push(api);
        self
    }

    pub fn consumer_push(self, consumer: fn(Self) -> Self) -> Self {
        consumer(self)
    }

    fn omitted_path(&self, path: &'static str) -> String {
        let parts = path.split(PATH_PART_SEPARATOR).collect::<Vec<_>>();
        parts.iter().enumerate()
            // filtered out omitted part in path and join the other into a new one
            .filter(|(i, _)| ! self.raw_omitted_paths.contains(parts[0..=*i].join(PATH_PART_SEPARATOR.to_string().as_str()).as_str()))
            .map(|t| t.1.to_string())
            .collect::<Vec<_>>()
            .join(PATH_PART_SEPARATOR.to_string().as_str())
    }

    fn closing(&mut self) -> DceResult<()> {
        self.build_tree()?;
        while let Some(api) = self.api_buffer.pop() {
            let path = self.omitted_path(api.path());
            let mut apis = vec![api];
            let mut suffixes = api.suffixes().clone();
            for index in (0..self.api_buffer.len()).rev() {
                if path.eq_ignore_ascii_case(self.omitted_path(&self.api_buffer[index].path()).as_str()) {
                    // push to vec if omitted path are same
                    let sibling_api = self.api_buffer.remove(index);
                    suffixes.extend(sibling_api.suffixes().clone());
                    apis.insert(0, sibling_api);
                }
            }
            // Append suffix to path as api mapping key to grouping the apis
            for suffix in suffixes {
                let suffixed_path = self.suffix_path(&path, &*suffix);
                self.apis_mapping.insert(Box::leak(suffixed_path.into_boxed_str()), apis.iter()
                    .filter(|api| api.suffixes().contains(&suffix))
                    .map(|api| *api)
                    .collect::<Vec<_>>());
            }
        }
        Ok(())
    }

    fn suffix_path(&self, path: &String, suffix: &str) -> String {
        format!("{}{}", path, if suffix.is_empty() { "".to_owned() } else { format!("{}{}", SUFFIX_BOUNDARY, suffix) })
    }

    fn build_tree(&mut self) -> DceResult<()> {
        let suffix_less_apis_groups: Vec<Vec<_>> = self.api_buffer.iter().map(|a| a.path()).collect::<HashSet<_>>()
            .iter().map(|path| self.api_buffer.iter().filter_map(|api| if api.path().eq(*path) { Some(*api) } else { None }).collect()).collect();
        // 1. init the apis_tree
        self.apis_tree.build(
            suffix_less_apis_groups.iter().map(|apis| ApiBranch::new(apis[0].path(), apis.clone())).collect::<Vec<_>>(),
            // if there had some remains api
            // fill parent ApiBranches, for example only configured one api with path ["home/common/index"],
            // then it will be filled to ["home", "home/common", "home/common/index"]
            Some(|tree: &ATree<ApiBranch<Rp>, &'static str>, mut remains: Vec<ApiBranch<Rp>>| {
                let mut fills: BTreeMap<Vec<&'static str>, ApiBranch<Rp>> = BTreeMap::new();
                while let Some(element) = remains.pop() {
                    let paths: Vec<_> = element.path.split(PATH_PART_SEPARATOR).collect();
                    for i in 0..paths.len() - 1 {
                        let path = paths[..=i].to_vec();
                        if matches!(tree.get_by_path(&path), None) && ! fills.contains_key(&path) {
                            // the missed branch must be bare, so the apis should be an empty vec
                            let api_path = ApiBranch::new(Box::leak(path.clone().join(PATH_PART_SEPARATOR.to_string().as_str()).into_boxed_str()), vec![]);
                            fills.insert(path, api_path);
                        }
                    }
                    // the remains element self should directly insert into
                    fills.insert(paths, element);
                }
                while let Some((paths, nb)) = fills.pop_first() {
                    let _ = tree.set_by_path(paths, nb);
                }
            }),
        )?;
        // 2. fill the tree item properties
        self.apis_tree.traversal(|tree| {
            let is_var_elem = ! matches!(tree.read().map_err(DceErr::closed0)?.var_type, VarType::NotVar);
            let mut current = tree.clone();
            let mut is_omitted_passed_child = false;
            while let Some(parent) = current.parent() {
                if ! parent.read().map_err(DceErr::closed0)?.is_omission {
                    let mut parent_writable = parent.write().map_err(DceErr::closed0)?;
                    match parent_writable.var_type {
                        VarType::Required(_) => parent_writable.is_mid_var = true,
                        VarType::NotVar => {},
                        _ => panic!("Ambiguous type var '{}' cannot in middle.", parent_writable.key()),
                    }
                    // push to var_children if is a var whatever is it an omitted_passed_child or not
                    if is_var_elem {
                        parent_writable.var_children.push(tree.clone());
                    } else if is_omitted_passed_child {
                        parent_writable.omitted_passed_children.insert(tree.read().map_err(DceErr::closed0)?.key(), tree.clone());
                    }
                    break;
                }
                is_omitted_passed_child = true;
                current = parent;
            }
            Ok(TreeTraverBreak::Continue)
        })
    }

    pub fn ready(mut self) -> DceResult<&'static Arc<Router<Rp>>> {
        self.closing()?;
        Ok(Box::leak(Box::new(Arc::new(self))))
    }

    fn locate(
        &self,
        mut path: &str,
        api_finder: impl Fn(&Vec<&'static (dyn ApiTrait<Rp> + Send + Sync)>) -> DceResult<&'static (dyn ApiTrait<Rp> + Send + Sync)>,
    ) -> DceResult<(&'static (dyn ApiTrait<Rp> + Send + Sync), HashMap<&'static str, PathParam>, Option<&'static str>)> {
        let request_path = path;
        let mut api;
        let mut path_args = Default::default();
        let mut suffix = None;
        // match api follow redirect
        loop {
            let mut apis = self.apis_mapping.get(path);
            if let Some((tmp_path, tmp_path_args, tmp_suffix)) = if apis.is_some() { None } else { self.match_var_path(path) } {
                // when directly matched in api_mapping, means the suffix must be matched too, do not extract it here to maximize performance
                // when var_path matched, means the suffix already matched and extracted, just pass to use
                apis = self.apis_mapping.get(self.suffix_path(&self.omitted_path(tmp_path), tmp_suffix).as_str());
                (path_args, suffix) = (tmp_path_args, Some(tmp_suffix));
            }
            api = api_finder(apis.ok_or_else(|| DceErr::openly(CODE_NOT_FOUND, format!(r#"path "{}" route failed, could not matched by Router"#, path)))?)?;
            if api.redirect().is_empty() {
                break;
            }
            path = api.redirect();
        }
        debug!(r#"{}: path "{}" matched api "{}""#, type_name::<Rp>(), request_path, api.path());
        Ok((api, path_args, suffix))
    }

    fn match_var_path(
        &self,
        path: &str,
    ) -> Option<(&'static str, HashMap<&'static str, PathParam>, &'static str)> {
        let path_parts = path.split(self.path_part_separator).collect::<Vec<_>>();
        let mut loop_items = vec![(self.apis_tree.clone(), 0_usize)];
        let mut target_api_branch = None;
        let mut suffix = "";
        let mut path_args = HashMap::new();
        'outer: while let Some((api_branch, part_number)) = loop_items.pop() {
            let is_last_part = part_number == path_parts.len() - 1;
            let is_overflowed = part_number >= path_parts.len();
            if is_overflowed && ! api_branch.read().ok()?.apis.is_empty() {
                // should be finished at last request path part if not a bare tree
                target_api_branch = Some(api_branch.clone());
                break;
            }
            // if not overflow and request path matched, then it must be a normal path
            if let Some((sub_api_branch, matched_suffix)) = if is_overflowed { None } else {
                self.find_consider_suffix(&path_parts[part_number], is_last_part, api_branch.children().read().ok()?, &api_branch.read().ok()?.omitted_passed_children)
            } {
                // push it into loop queue to handle it next cycle
                loop_items.push((sub_api_branch.clone(), 1 + part_number));
                suffix = matched_suffix;
            } else {
                let insert_pos = loop_items.len();
                for var_api_branch in api_branch.read().ok()?.var_children.clone() {
                    let var_api_branch_read = var_api_branch.read().ok()?;
                    if ! var_api_branch_read.is_mid_var {
                        // just need to check is_last_part because should already handle suffix if overflowed
                        // pop out the last part to clean (cut off the suffix)
                        let mut suffix_extractor = |path_parts: Vec<&str>, consumer: &mut dyn FnMut(Vec<&str>) -> Option<PathParam>| -> Option<PathParam> {
                            let mut path_parts = path_parts.clone();
                            if let Some(mut last_part) = path_parts.pop() {
                                // try match suffix in the last part, cut off it and the remains is the pure path parameter
                                // merge suffixes into a new BTreeSet to match in the order of complex suffix at the top
                                if let Some(tmp_suffix) = var_api_branch_read.apis.iter().flat_map(|api| api.suffixes()).collect::<BTreeSet<_>>()
                                    .iter().find(|suf| last_part.ends_with(format!("{}{}", self.suffix_boundary, &****suf).as_str())) {
                                    last_part = &last_part[0..last_part.len() - tmp_suffix.len() - 1];
                                    suffix = &**tmp_suffix;
                                }
                                path_parts.push(last_part);
                            }
                            consumer(path_parts)
                        };
                        // if not a middle var, then should finish var path match and collect vars and end the outer loop
                        match var_api_branch_read.var_type {
                            // should be a none optional parameter if it's overflowed
                            VarType::Optional(var_name) if is_overflowed => path_args.insert(var_name, PathParam::Option(None)),
                            // should be a some optional parameter if it's not overflowed
                            VarType::Optional(var_name) if is_last_part =>
                                suffix_extractor(path_parts, &mut |pps| path_args.insert(var_name, PathParam::Option(Some(pps[part_number].to_string())))),
                            VarType::Required(var_name) if is_last_part =>
                                suffix_extractor(path_parts, &mut |pps| path_args.insert(var_name, PathParam::Required(pps[part_number].to_string()))),
                            VarType::EmptableVector(var_name) if is_overflowed => path_args.insert(var_name, PathParam::Vector(vec![])),
                            // shouldn't be a valid vector if it's overflowed
                            VarType::EmptableVector(var_name) | VarType::Vector(var_name) if !is_overflowed =>
                                suffix_extractor(path_parts, &mut |pps| path_args.insert(var_name, PathParam::Vector(pps[part_number..].iter().map(|p| p.to_string()).collect::<Vec<_>>()))),
                            // if it should be the end vars but overflowed, continue the for loop to let other var api path to match
                            _ => continue,
                        };
                        target_api_branch = Some(var_api_branch.clone());
                        break 'outer;
                    } else if let VarType::Required(var_name) = var_api_branch_read.var_type {
                        // if it's middle var then insert to loop queue to handle it next cycle
                        path_args.insert(var_name, PathParam::Required(path_parts[part_number].to_string()));
                        loop_items.insert(insert_pos, (var_api_branch.clone(), 1 + part_number));
                    }
                }
            }
        }
        target_api_branch?.read().map(|branch| (branch.path, path_args, suffix)).ok()
    }

    fn find_consider_suffix(
        &self,
        part: &str,
        is_last_part: bool,
        children: RwLockReadGuard<BTreeMap<&'static str, Arc<ATree<ApiBranch<Rp>, &'static str>>>>,
        omitted_passed_children: &BTreeMap<&'static str, Arc<ATree<ApiBranch<Rp>, &'static str>>>,
    ) -> Option<(Arc<ATree<ApiBranch<Rp>, &'static str>>, &'static str)> {
        let matches = children.get(part).or_else(|| omitted_passed_children.get(part));
        if matches.is_none() && is_last_part {
            let mut boundary = part.len();
            while let Some(previous) = part[0..boundary].rfind(self.suffix_boundary) {
                // try to recursive match in children and omitted passed children, if matched then remains was the suffix
                let matches = children.get(&part[0..previous]).or_else(|| omitted_passed_children.get(&part[0..previous]));
                if let Some(matches) = matches {
                    return matches.read().ok()?.apis.iter()
                        .flat_map(|api| api.suffixes())
                        .find(|suffix| part[previous + 1 ..].eq(&***suffix))
                        .map(|suffix| (matches.clone(), &**suffix));
                }
                boundary = previous;
            }
        }
        // whatever is middle part matched or directly tail matched, the suffix should be empty
        return matches.map(|tree| (tree.clone(), ""));
    }

    #[cfg(feature = "async")]
    async fn routed_handle(result: DceResult<(&'static (dyn ApiTrait<Rp> + Send + Sync), HashMap<&'static str, PathParam>, Option<&'static str>)>, context: &mut Context<Rp>) -> DceResult<()> {
        let (api, path_args, suffix) = result?;
        context.set_routed_info(api, path_args, suffix);
        api.call_controller(context).await
    }

    #[cfg(not(feature = "async"))]
    fn routed_handle(result: DceResult<(&'static (dyn ApiTrait<Rp> + Send + Sync), HashMap<&'static str, PathParam>, Option<&'static str>)>, context: &mut Context<Rp>) -> DceResult<()> {
        let (api, path_args, suffix) = result?;
        context.set_routed_info(api, path_args, suffix);
        api.call_controller(context)
    }

    #[cfg(feature = "async")]
    pub async fn route(context: &mut Context<Rp>) -> DceResult<()> {
        Self::routed_handle(context.router().locate(context.rp().path(), |apis| context.rp().api_match(apis)), context).await
    }

    #[cfg(not(feature = "async"))]
    pub fn route(context: &mut Context<Rp>) -> DceResult<()> {
        Self::routed_handle(context.router().locate(context.rp().path(), |apis| context.rp().api_match(apis)), context)
    }

    fn id_locate(&self, id: &str) -> DceResult<(&'static (dyn ApiTrait<Rp> + Send + Sync), HashMap<&'static str, PathParam>, Option<&'static str>)> {
        self.id_api_mapping.get(id).map_or_else(
            || Err(DceErr::openly(CODE_NOT_FOUND, format!(r#"id "{}" route failed, could not matched by Router"#, id))),
            |api| {
                debug!(r#"{}: id "{}" matched api "{}""#, type_name::<Rp>(), id, api.path());
                Ok((*api, Default::default(), None))
            })
    }

    #[cfg(feature = "async")]
    pub async fn id_route(context: &mut Context<Rp>) -> DceResult<()> {
        Self::routed_handle(context.router().id_locate(context.rp().path()), context).await
    }

    #[cfg(not(feature = "async"))]
    pub fn id_route(context: &mut Context<Rp>) -> DceResult<()> {
        Self::routed_handle(context.router().id_locate(context.rp().path()), context)
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
pub struct ApiBranch<Rp: RoutableProtocol + 'static> {
    path: &'static str,
    var_type: VarType,
    is_mid_var: bool,
    is_omission: bool,
    apis: Vec<&'static (dyn ApiTrait<Rp> + Send + Sync)>,
    var_children: Vec<Arc<ATree<ApiBranch<Rp>, &'static str>>>,
    omitted_passed_children: BTreeMap<&'static str, Arc<ATree<ApiBranch<Rp>, &'static str>>>,
}

impl<Rp: RoutableProtocol> ApiBranch<Rp> {
    fn new(path: &'static str, apis: Vec<&'static (dyn ApiTrait<Rp> + Send + Sync)>) -> ApiBranch<Rp> {
        ApiBranch {
            path,
            var_type: VarType::NotVar,
            is_mid_var: false,
            is_omission: apis.iter().any(|api| api.omission()),
            apis,
            var_children: Default::default(),
            omitted_passed_children: Default::default(),
        }.fill_var_type()
    }

    fn fill_var_type(mut self) -> ApiBranch<Rp> {
        let key = self.key();
        if key.starts_with(VARIABLE_OPENER) && key.ends_with(VARIABLE_CLOSING) {
            if self.is_omission {
                panic!("Var path could not be omissible.");
            }
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

impl<Rp: RoutableProtocol> KeyFactory<&'static str> for ApiBranch<Rp> {
    fn key(&self) -> &'static str {
        self.path.rfind(PATH_PART_SEPARATOR).map_or_else(|| self.path.trim(), |index| &self.path[index+1 ..].trim())
    }

    fn child_of(&self, parent: &Self) -> bool {
        self.path.rfind(PATH_PART_SEPARATOR).map_or_else(|| parent.path.is_empty(), |index| &self.path[..index] == parent.path)
    }
}

impl<Rp: RoutableProtocol> PartialEq for ApiBranch<Rp> {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}
