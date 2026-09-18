//! Self-sends inside block/lambda bodies are never a conclusive E0101:
//! the method receiving the block may `instance_exec`/`instance_eval` it
//! against another object, rebinding `self` while leaving locals intact.
//! Found on the corpus after the sig carve-out (bead ita-4xy) closed
//! DSL classes that rebind field blocks against a context object.
//!
//! Both sides proven: the same misspelled self-send accuses OUTSIDE a
//! block (the mutation guard — softening too broadly would kill it) and
//! stays silent INSIDE one; explicit-receiver calls inside blocks keep
//! full checking because rebinding never touches locals.

use itaruby_semantic::{check_file, Db, ProjectFiles, SourceFile};

fn diagnostics_for(src: &str) -> Vec<String> {
    let db = Db::new();
    let file = SourceFile::new(&db, "block_self_send_probe.rb".into(), src.to_string());
    ProjectFiles::new(&db, vec![file]);
    check_file(&db, file)
        .iter()
        .map(|d| d.message.clone())
        .collect()
}

const CLOSED_CLASS: &str = "
class BlkSelfWidget
  def run
    helper { misspelled_helper_call }
  end

  def helper
    yield
  end
end
";

const CONTROL_OUTSIDE_BLOCK: &str = "
class BlkSelfControl
  def run
    misspelled_helper_call
  end
end
";

const EXPLICIT_RECEIVER_IN_BLOCK: &str = "
class BlkSelfRecv
  def run
    other = BlkSelfRecvOther.new
    helper { other.misspelled_on_receiver }
  end

  def helper
    yield
  end
end

class BlkSelfRecvOther
  def real_method
  end
end
";

const LAMBDA_SELF_SEND: &str = "
class BlkSelfLambda
  def run
    callback = -> { misspelled_in_lambda }
    callback
  end
end
";

#[test]
fn self_send_typo_inside_block_is_silent() {
    let diags = diagnostics_for(CLOSED_CLASS);
    assert!(
        diags.is_empty(),
        "self-send in a block must never be conclusive (instance_exec rebinding): {diags:?}"
    );
}

#[test]
fn same_typo_outside_block_still_accuses() {
    let diags = diagnostics_for(CONTROL_OUTSIDE_BLOCK);
    assert!(
        diags.iter().any(|m| m.contains("misspelled_helper_call")),
        "softening must be block-scoped only; outside a block the E0101 stands: {diags:?}"
    );
}

#[test]
fn explicit_receiver_inside_block_still_accuses() {
    let diags = diagnostics_for(EXPLICIT_RECEIVER_IN_BLOCK);
    assert!(
        diags.iter().any(|m| m.contains("misspelled_on_receiver")),
        "rebinding only changes self — receiver-carrying calls in blocks stay checked: {diags:?}"
    );
}

#[test]
fn self_send_typo_inside_lambda_is_silent() {
    let diags = diagnostics_for(LAMBDA_SELF_SEND);
    assert!(
        diags.is_empty(),
        "lambdas can be instance_exec'd too — same softening applies: {diags:?}"
    );
}
