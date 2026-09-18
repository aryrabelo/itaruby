class IvarNoAssignWidget
  # `@gadget` is read here but never assigned anywhere in this class body
  # — the type must stay Unknown, never an error.
  def broken
    @gadget.nonexistent_method
  end
end
