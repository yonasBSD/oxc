use oxc_ast::AstKind;
use oxc_cfg::BasicBlockId;
use oxc_index::IndexVec;
pub use oxc_syntax::node::{AstNodeId, NodeFlags};

use crate::scope::ScopeId;

/// Semantic node contains all the semantic information about an ast node.
#[derive(Debug, Clone, Copy)]
pub struct AstNode<'a> {
    id: AstNodeId,
    /// A pointer to the ast node, which resides in the `bumpalo` memory arena.
    kind: AstKind<'a>,

    /// Associated Scope (initialized by binding)
    scope_id: ScopeId,

    /// Associated `BasicBlockId` in CFG (initialized by control_flow)
    cfg_id: BasicBlockId,

    flags: NodeFlags,
}

impl<'a> AstNode<'a> {
    pub(crate) fn new(
        kind: AstKind<'a>,
        scope_id: ScopeId,
        cfg_id: BasicBlockId,
        flags: NodeFlags,
        id: AstNodeId,
    ) -> Self {
        Self { id, kind, scope_id, cfg_id, flags }
    }

    pub fn id(&self) -> AstNodeId {
        self.id
    }

    pub fn cfg_id(&self) -> BasicBlockId {
        self.cfg_id
    }

    pub fn kind(&self) -> AstKind<'a> {
        self.kind
    }

    pub fn scope_id(&self) -> ScopeId {
        self.scope_id
    }

    pub fn flags(&self) -> NodeFlags {
        self.flags
    }

    pub fn flags_mut(&mut self) -> &mut NodeFlags {
        &mut self.flags
    }
}

/// Untyped AST nodes flattened into an vec
#[derive(Debug, Default)]
pub struct AstNodes<'a> {
    /// The root node should always point to a `Program`, which is the real
    /// root of the tree. It isn't possible to statically check for this, so
    /// users should beware.
    root: Option<AstNodeId>,
    nodes: IndexVec<AstNodeId, AstNode<'a>>,
    parent_ids: IndexVec<AstNodeId, Option<AstNodeId>>,
}

impl<'a> AstNodes<'a> {
    pub fn iter(&self) -> impl Iterator<Item = &AstNode<'a>> + '_ {
        self.nodes.iter()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.len() == 0
    }

    /// Walk up the AST, iterating over each parent node.
    ///
    /// The first node produced by this iterator is the first parent of the node
    /// pointed to by `node_id`. The last node will usually be a `Program`.
    pub fn iter_parents(&self, node_id: AstNodeId) -> impl Iterator<Item = &AstNode<'a>> + '_ {
        let curr = Some(self.get_node(node_id));
        AstNodeParentIter { curr, nodes: self }
    }

    pub fn kind(&self, ast_node_id: AstNodeId) -> AstKind<'a> {
        self.nodes[ast_node_id].kind
    }

    pub fn parent_id(&self, ast_node_id: AstNodeId) -> Option<AstNodeId> {
        self.parent_ids[ast_node_id]
    }

    pub fn parent_kind(&self, ast_node_id: AstNodeId) -> Option<AstKind<'a>> {
        self.parent_id(ast_node_id).map(|node_id| self.kind(node_id))
    }

    pub fn parent_node(&self, ast_node_id: AstNodeId) -> Option<&AstNode<'a>> {
        self.parent_id(ast_node_id).map(|node_id| self.get_node(node_id))
    }

    pub fn get_node(&self, ast_node_id: AstNodeId) -> &AstNode<'a> {
        &self.nodes[ast_node_id]
    }

    pub fn get_node_mut(&mut self, ast_node_id: AstNodeId) -> &mut AstNode<'a> {
        &mut self.nodes[ast_node_id]
    }

    /// Get the root `AstNodeId`, It is always pointing to a `Program`.
    /// Returns `None` if root node isn't set.
    pub fn root(&self) -> Option<AstNodeId> {
        self.root
    }

    /// Get the root node as immutable reference, It is always guaranteed to be a `Program`.
    /// Returns `None` if root node isn't set.
    pub fn root_node(&self) -> Option<&AstNode<'a>> {
        self.root().map(|id| self.get_node(id))
    }

    /// Get the root node as mutable reference, It is always guaranteed to be a `Program`.
    /// Returns `None` if root node isn't set.
    pub fn root_node_mut(&mut self) -> Option<&mut AstNode<'a>> {
        self.root().map(|id| self.get_node_mut(id))
    }

    /// Walk up the AST, iterating over each parent node.
    ///
    /// The first node produced by this iterator is the first parent of the node
    /// pointed to by `node_id`. The last node will usually be a `Program`.
    pub fn ancestors(&self, ast_node_id: AstNodeId) -> impl Iterator<Item = AstNodeId> + '_ {
        let parent_ids = &self.parent_ids;
        std::iter::successors(Some(ast_node_id), |node_id| parent_ids[*node_id])
    }

    /// Create and add an `AstNode` to the `AstNodes` tree and returns its `AstNodeId`.
    /// Node must not be `Program`. Use `add_program_node` instead.
    pub fn add_node(
        &mut self,
        kind: AstKind<'a>,
        scope_id: ScopeId,
        parent_node_id: AstNodeId,
        cfg_id: BasicBlockId,
        flags: NodeFlags,
    ) -> AstNodeId {
        let ast_node_id = self.parent_ids.push(Some(parent_node_id));
        let node = AstNode::new(kind, scope_id, cfg_id, flags, ast_node_id);
        self.nodes.push(node);
        ast_node_id
    }

    /// Create and add an `AstNode` to the `AstNodes` tree and returns its `AstNodeId`.
    pub fn add_program_node(
        &mut self,
        kind: AstKind<'a>,
        scope_id: ScopeId,
        cfg_id: BasicBlockId,
        flags: NodeFlags,
    ) -> AstNodeId {
        let ast_node_id = self.parent_ids.push(None);
        self.root = Some(ast_node_id);
        let node = AstNode::new(kind, scope_id, cfg_id, flags, ast_node_id);
        self.nodes.push(node);
        ast_node_id
    }

    pub fn reserve(&mut self, additional: usize) {
        self.nodes.reserve(additional);
        self.parent_ids.reserve(additional);
    }
}

#[derive(Debug)]
pub struct AstNodeParentIter<'s, 'a> {
    curr: Option<&'s AstNode<'a>>,
    nodes: &'s AstNodes<'a>,
}

impl<'s, 'a> Iterator for AstNodeParentIter<'s, 'a> {
    type Item = &'s AstNode<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.curr;
        self.curr = self.curr.and_then(|curr| self.nodes.parent_node(curr.id()));

        next
    }
}