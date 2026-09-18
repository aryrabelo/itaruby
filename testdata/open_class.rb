class Widget
  def method_missing(name, *args)
    nil
  end
end

Widget.new.nonexistent_method
