 SOM Module System Implementation Guide

  1. Overview & Design Decisions

  Your Design Goals

  You've chosen a module system with these properties:

  1. Folders = Modules: Each folder is a module namespace (e.g., std/ is the std module)
  2. Multiple Files per Module: All .som files in a folder belong to the same module
  3. File-Scoped Privacy by Default: Items without pub are private to their file
  4. Three Visibility Levels:
    - (no modifier) = file-private
    - pub(mod) = module-internal (visible in folder)
    - pub = public (exported from module)
  5. Circular Dependencies Allowed: Modules can import each other

  Why These Design Choices Matter

  Why folders = modules?

- Clean organization: related code stays together
- Clear namespace boundaries
- Easy to find code (path matches namespace)

  Why file-scoped privacy?

- Encourages small, focused files
- Better encapsulation (can't accidentally use internals)
- Files are independent units

  Why allow circular dependencies?

- More flexible (real projects have cycles)
- Mirrors how types actually work (structs can reference each other)
- Less restrictive for developers

  ---

  2. The Core Problem: Circular Dependencies

  Why We Can't Just Type Check Linearly

  Consider this example:

  // std/types.som
  use std::utils;

  pub type Config = {
      validator ~ Validator  // References type from std::utils
  };

  // std/utils.som
  use std::types;

  pub type Validator = {
      config ~ Config  // References type from std::types
  };

  The problem:

- To type check std/types, we need to know what Validator is
- To type check std/utils, we need to know what Config is
- They depend on each other! Neither can go first

  The Solution: Forward Declarations

  The solution is to split type checking into phases:

  1. First: Tell the type checker "a type called Config exists" (but don't define it yet)
  2. Second: Tell it "a type called Validator exists"
  3. Third: Now fill in what Config actually is (we can reference Validator because we know it exists)
  4. Fourth: Fill in what Validator is (we can reference Config because we know it exists)

  This is called forward declaration - declaring that something exists before defining what it is.

  ---

  3. Data Structures

  TypeCheckerScope

  pub struct TypeCheckerScope<'a> {
      parent: Option<&'a TypeCheckerScope<'a>>,
      types: HashMap<String, Type>,
      variables: HashMap<String, Type>,
      kind: ScopeKind,
  }

  pub enum ScopeKind {
      Module,    // Module-level scope (folder)
      File,      // File-level scope
      Function,  // Function scope
      Block,     // Block scope
  }

  Why do we need this?

- Manages what's visible at different levels
- File scope can see module scope (its parent)
- Module scope can see imported modules (its parent)
- Natural lookup: check current scope, then parent, then grandparent, etc.

  Example hierarchy:
  Global Scope (all imported modules)
    └─ Module Scope (std module: pub + pub(mod) items)
        ├─ File Scope (std/io.som: private items)
        └─ File Scope (std/string.som: private items)

  ModuleScope

  pub struct ModuleScope {
      // Exported items (pub only) - visible to OTHER modules
      pub public_types: HashMap<String, Type>,
      pub public_variables: HashMap<String, Type>,

      // Internal items (pub + pub(mod)) - visible WITHIN module
      pub module_types: HashMap<String, Type>,
      pub module_variables: HashMap<String, Type>,
  }

  Why two sets of items?

- When another module does use std;, they only get public_* items
- When a file in the std module looks up a name, it gets module_* items
- This enforces visibility boundaries

  Module and File

  pub struct Module<P: Phase> {
      pub path: Path,           // e.g., ["std", "io"]
      pub files: Vec<File<P>>,  // All .som files in std/io/ folder
  }

  pub struct File<P: Phase> {
      pub declarations: Vec<Declaration<P>>,
      pub span: Span,
  }

  Visibility

  pub enum Visibility {
      Private,  // No modifier - file only
      Module,   // pub(mod) - folder only
      Public,   // pub - exported
  }

  ---

  4. The Three-Pass Algorithm

  Why Three Passes?

  Pass 1 solves: "I need to know this type exists before I define it"
  Pass 2 solves: "Now I can define it because all names are known"
  Pass 3 solves: "Now I can type check code that uses those types"

  Pass 1: Collect Type Names (Forward Declarations)

  Goal: Register every public and module-internal type name, but don't resolve their bodies yet.

  What we do:
  for each module:
      for each file in module:
          for each type definition:
              if visibility is Public or Module:
                  add name to module registry with placeholder type

  Example:

  Before Pass 1:
  // Registry is empty
  {}

  After Pass 1:
  // Registry now knows these type names exist:
  {
      ["std", "types"]: {
          public_types: { "Config": Forward("Config") },
          module_types: { "Config": Forward("Config") }
      },
      ["std", "utils"]: {
          public_types: { "Validator": Forward("Validator") },
          module_types: { "Validator": Forward("Validator") }
      }
  }

  Forward("Config") is a placeholder that says "this type exists but we don't know what it is yet".

  Why this matters:
  Now when we're defining Config, if we encounter the name Validator, we can look it up and find "oh yes, that's a valid type that exists somewhere". We
  don't need to know what it is yet, just that it exists.

  Pass 2: Resolve Type Bodies

  Goal: Fill in the actual definitions of types, now that all names are registered.

  What we do:
  for each module:
      create scope with module-internal items from Pass 1

      for each file in module:
          for each type definition:
              resolve the type body (can reference other types now!)
              update module registry with resolved type

  Example:

  Before Pass 2:
  // Config is just a forward declaration
  "Config": Forward("Config")

  During Pass 2:
  // Resolving Config's body:
  pub type Config = {
      validator ~ Validator  // Look up "Validator"
                            // Found: Forward("Validator") - OK, it exists!
  };

  // After resolution:
  "Config": Struct {
      fields: [
          { name: "validator", type: Forward("Validator") }
      ]
  }

  After Pass 2 completes for all modules:
  // All types are fully resolved:
  "Config": Struct {
      fields: [
          { name: "validator", type: Struct { ... } }  // Fully resolved
      ]
  }

  "Validator": Struct {
      fields: [
          { name: "config", type: Struct { ... } }  // Fully resolved
      ]
  }

  Why this matters:

- All type references are resolved
- Circular type dependencies are handled
- We can now type check variables and functions

  Pass 3: Type Check Everything Else

  Goal: Type check variable declarations, function bodies, expressions.

  What we do:
  for each module:
      create scope with fully resolved types from Pass 2

      for each file in module:
          create file scope as child of module scope
          add file-private items to file scope

          for each declaration:
              type check with complete type information

  Example:

  // std/types.som
  pub let default_config ~ Config = {
      validator: default_validator()  // Type check this expression
  };

  // Type checking:
  // 1. Look up Config - found in module scope ✓
  // 2. Type check the struct construction
  // 3. Check field types match

  Why this matters:

- All type information is complete
- Normal type checking can proceed
- No missing type definitions

  ---

  5. Visibility System in Detail

  The Three Levels

  // std/io.som
  fn helper() { ... }              // Private - only std/io.som
  pub(mod) fn internal() { ... }   // Module - any file in std/
  pub fn println() { ... }         // Public - exported from std

  // std/string.som (same module - different file)
  fn split() {
      helper();     // ❌ ERROR - helper is file-private to io.som
      internal();   // ✅ OK - pub(mod) visible in same module
      println();    // ✅ OK - pub is visible everywhere
  }

  // main.som (different module)
  use std;

  fn main() {
      helper();     // ❌ ERROR - not exported
      internal();   // ❌ ERROR - not exported
      println();    // ✅ OK - pub was exported
  }

  How Scope Lookup Works

  When looking up a name foo:

  1. Check file scope - is foo defined in this file?
  2. Check module scope - is foo a pub(mod) or pub item from another file in this module?
  3. Check imported scopes - did we use a module that exports foo?
  4. Not found - error

  Example:

  // In std/string.som, looking up "internal":

  File Scope (std/string.som)
  ├─ No "internal" here
  │
  └─ parent: Module Scope (std)
     ├─ module_variables: { "internal": ... }  ← Found it!
     └─ parent: Global Scope

  Building Scopes for Type Checking

  Step 1: Build Module Scope

  Collect all pub and pub(mod) items from ALL files in the module:

  fn build_module_scope(module: &Module<Untyped>) -> TypeCheckerScope {
      let mut scope = TypeCheckerScope::new(ScopeKind::Module);

      for file in &module.files {
          for decl in &file.declarations {
              match decl {
                  Declaration::Type(t) if t.visibility != Visibility::Private => {
                      scope.declare_type(t.name, t.ty);
                  }
                  Declaration::Let(v) if v.visibility != Visibility::Private => {
                      scope.declare_variable(v.name, infer_type(&v.value));
                  }
                  _ => {}
              }
          }
      }

      scope
  }

  Step 2: Build File Scope

  Create a child scope with file-private items:

  fn build_file_scope<'a>(
      file: &File<Untyped>,
      module_scope: &'a TypeCheckerScope
  ) -> TypeCheckerScope<'a> {
      let mut file_scope = module_scope.new_child(ScopeKind::File);

      for decl in &file.declarations {
          match decl {
              Declaration::Type(t) if t.visibility == Visibility::Private => {
                  file_scope.declare_type(t.name, t.ty);
              }
              Declaration::Let(v) if v.visibility == Visibility::Private => {
                  file_scope.declare_variable(v.name, infer_type(&v.value));
              }
              _ => {}
          }
      }

      file_scope
  }

  Step 3: Type Check File

  fn typecheck_file(
      file: File<Untyped>,
      module_scope: &TypeCheckerScope,
  ) -> Result<File<Typed>> {
      let file_scope = build_file_scope(&file, module_scope);
      let mut ctx = TypeCheckContext::new(file_scope);

      file.type_check(&mut ctx)
  }

  ---

  6. Import Resolution

  What use std; Does

  impl TypeCheck for Import {
      fn type_check(self, ctx: &mut TypeCheckContext) -> Result<Self> {
          // 1. Look up the module in the registry
          let module_scope = ctx.get_module_scope(&self.module)?;

          // 2. Add ONLY public items to current scope
          for (name, ty) in &module_scope.public_types {
              ctx.declare_type(name.clone(), ty.clone());
          }

          for (name, ty) in &module_scope.public_variables {
              ctx.declare_variable(name.clone(), ty.clone());
          }

          Ok(self)
      }
  }

  Key point: Imports only bring in public_*items, not module_* items.

  Example

  // std/io.som
  pub fn println() { ... }
  pub(mod) fn internal_logger() { ... }

  // main.som
  use std;

  println();          // ✅ Works - println is public
  internal_logger();  // ❌ Error - not exported

  ---

  7. Complete Walkthrough Example

  The Code

  // std/types.som
  pub type Config = {
      name ~ str,
      validator ~ Validator  // From std/utils
  };

  pub(mod) let internal_config ~ Config = { ... };

  // std/utils.som
  pub type Validator = {
      config ~ Config  // From std/types
  };

  pub fn validate(v ~ Validator) { ... }

  fn helper() { ... }  // Private

  // std/checks.som (same module)
  fn check() {
      let c = internal_config;  // OK - pub(mod)
      helper();  // ERROR - file-private to utils.som
  }

  // main.som
  use std;

  let v ~ Validator = ...;
  validate(v);  // OK - both are public

  Pass 1: Collect Type Names

  Action: Scan all files, register type names

  Result:
  module_registry = {
      ["std"]: ModuleScope {
          public_types: {
              "Config": Forward("Config"),
              "Validator": Forward("Validator")
          },
          module_types: {
              "Config": Forward("Config"),
              "Validator": Forward("Validator")
          },
          public_variables: {},
          module_variables: {}
      }
  }

  Why: Now when we're defining Config, we can reference Validator even though we haven't defined Validator yet.

  Pass 2: Resolve Type Bodies

  Action: Resolve each type definition

  For Config:
  // Creating scope for std module
  module_scope contains:
    - "Config": Forward("Config")
    - "Validator": Forward("Validator")

  // Resolving Config body:
  pub type Config = {
      name ~ str,           // Built-in, no lookup needed
      validator ~ Validator // Look up in scope → Found: Forward("Validator")
  };

  // Result: Config is now fully defined
  "Config": Struct {
      fields: [
          { name: "name", type: String },
          { name: "validator", type: Reference("Validator") }
      ]
  }

  For Validator:
  // Now Config is resolved, so this works:
  pub type Validator = {
      config ~ Config  // Look up → Found: Struct { ... }
  };

  "Validator": Struct {
      fields: [
          { name: "config", type: Reference("Config") }
      ]
  }

  Result after Pass 2:
  module_registry = {
      ["std"]: ModuleScope {
          public_types: {
              "Config": Struct { ... },      // Fully resolved
              "Validator": Struct { ... }     // Fully resolved
          },
          // ... module_variables populated too
      }
  }

  Pass 3: Type Check Declarations

  For std/types.som:

  // Create module scope (contains Config, Validator, etc.)
  module_scope = TypeCheckerScope {
      types: { "Config": ..., "Validator": ... },
      variables: { "internal_config": ... }
  }

  // Create file scope for types.som
  file_scope = module_scope.new_child(File)
  // (no file-private items in this example)

  // Type check:
  pub(mod) let internal_config ~ Config = { ... };
  // 1. Look up "Config" → Found in module scope ✓
  // 2. Type check the value
  // 3. Add "internal_config" to module_variables (it's pub(mod))

  For std/checks.som:

  // File scope for checks.som
  file_scope = module_scope.new_child(File)

  // Type check:
  fn check() {
      let c = internal_config;  // Look up "internal_config"
                                // → Found in module scope (pub(mod)) ✓

      helper();  // Look up "helper"
                 // → Not in file scope
                 // → Not in module scope (it's private to utils.som)
                 // → ERROR: undefined variable
  }

  For main.som:

  // First, process the import:
  use std;
  // → Add std's public_types and public_variables to main's scope

  // Type check:
  let v ~ Validator = ...;
  // Look up "Validator" → Found (was imported) ✓

  validate(v);
  // Look up "validate" → Found (was imported) ✓

  ---

  8. Implementation Checklist

  Phase 1: Data Structures (Do First)

  1. ✅ Add Visibility enum to all declaration types
  2. ✅ Create TypeCheckerScope struct
  3. ✅ Create ModuleScope struct with public/module separation
  4. ✅ Update TypeCheckContext to use scopes
  5. ✅ Add Type::Forward(String) variant for forward declarations

  Phase 2: Module Loading (Do Second)

  1. ✅ ProgramParser to find and parse all .som files
  2. ✅ Group files by folder into Module<Untyped>
  3. ✅ Build dependency graph from use statements

  Phase 3: Three-Pass Type Checker (Do Third)

  1. ✅ Pass 1 implementation:
    - Scan all modules
    - Register type names with Forward placeholders
    - Build initial ModuleScope for each module
  2. ✅ Pass 2 implementation:
    - For each module, create scope with forward declarations
    - Resolve each type definition
    - Update registry with resolved types
  3. ✅ Pass 3 implementation:
    - For each module, create complete module scope
    - For each file, create file scope
    - Type check declarations with full type information

  Phase 4: Import Handling (Do Fourth)

  1. ✅ Implement Import::type_check()
  2. ✅ Add only public_* items to importing scope
  3. ✅ Handle module not found errors

  Phase 5: Testing (Do Throughout)

  Test cases to write:

  1. Basic visibility:
    - File-private items not visible in other files
    - Module-internal items visible in same module
    - Public items visible after import
  2. Circular dependencies:
    - Two types that reference each other
    - Two modules that import each other
  3. Forward declarations:
    - Type using another type from same module
    - Type using another type from imported module
  4. Scope hierarchy:
    - Nested blocks
    - Function scopes
    - Module boundaries

  ---

  9. Common Pitfalls & Solutions

  Pitfall 1: Forgetting to Add Items to Module Scope

  Problem:
  // You type check a file but forget to add pub(mod) items to module scope
  // Other files in the module can't see them

  Solution: In Pass 1, always scan ALL files and collect ALL non-private items.

  Pitfall 2: Infinite Type Recursion

  Problem:
  pub type A = { field ~ A };  // Directly contains itself - impossible!

  Solution: Track recursion depth, or check that recursive references go through pointers:
  pub type A = { field ~ *A };  // OK - pointer breaks the cycle

  Pitfall 3: Import Before Type Checking

  Problem:
  // Trying to import a module before it's type-checked
  // Module registry doesn't have its types yet

  Solution: Type check in dependency order, or do all three passes for all modules before checking imports.

  Pitfall 4: Mixing Up Public vs Module Scope

  Problem:
  // Adding module_variables to imports (should be public_variables)

  Solution: Always use public_*for imports, module_* for same-module lookups.

  ---

  10. Summary: The Big Picture

  What Happens When You Compile

  1. Parse all .som files
     ↓
  2. Group into modules (by folder)
     ↓
  3. Pass 1: Register all type names
     ↓
  4. Pass 2: Resolve all type bodies
     ↓
  5. Pass 3: Type check everything
     ↓
  6. Code generation

  Key Insights

  1. Forward declarations solve circular dependencies - declare first, define later
  2. Scopes solve visibility - file → module → global hierarchy
  3. Multiple passes solve ordering - can't do everything at once
  4. Separation of public/module items enforces boundaries - imports only get public stuff

  The Mental Model

  Think of it like building a house:

- Pass 1: Put up the frame (structure exists, but empty)
- Pass 2: Install walls and doors (structure is complete)
- Pass 3: Add furniture and decorations (everything works)

  You can't add furniture before the walls exist, and you can't install walls before the frame exists. Same with type checking!
