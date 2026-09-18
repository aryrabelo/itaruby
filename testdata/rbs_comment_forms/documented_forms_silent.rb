# typed: strict
# frozen_string_literal: true

# Bead ita-p24: every documented sorbet.org/docs/rbs-support `#:` form that
# was previously rejected as E0105 (bare arrow, named params, postfix
# nilable, rest params, block/proc types, generic type params). Must stay
# silent — see `rbs_comments.rs`'s `documented_forms_never_warn_e0105`.
class RbsCommentFormsProbe
  #: -> void
  def bare_arrow_no_params
  end

  #: -> String?
  def bare_arrow_postfix_nilable
    nil
  end

  #: (Class desired_class, ?String? desired_method, ?id: Integer?) -> untyped
  def positional_and_keyword_names(desired_class, desired_method = nil, id: nil)
    desired_class
  end

  #: (*String args) -> void
  def rest_positional(*args)
  end

  #: (String project_path, **untyped options) -> void
  def rest_keyword(project_path, **options)
  end

  #: (String module_name) { (Integer index, String base) -> void } -> void
  def required_block(module_name)
    yield(1, module_name)
  end

  #: (String? query) ?{ (String) -> bool? } -> Array[String]
  def optional_block(query)
    []
  end

  #: { -> Integer } -> Integer
  def bare_block_no_params
    yield
  end

  #: [T] (String request_name, T value) -> T
  def generic_method(request_name, value)
    value
  end

  #: (^(Integer arg0) -> Integer) -> void
  def proc_type_param(callback)
    callback.call(1)
  end

  #: (String uri) -> (Array[String] | Array[Integer])?
  def parenthesized_union_return(uri)
    nil
  end

  #: (String name) -> [String, String]
  def tuple_return(name)
    [name, name]
  end

  #: (String cop_name) -> singleton(RbsCommentFormsProbe)?
  def singleton_return(cop_name)
    nil
  end
end
