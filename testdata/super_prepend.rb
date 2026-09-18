class SuperAnnouncer
  def greet
    'hi'
  end
end

module SuperLoudAnnounce
  def greet
    "LOUD: #{super}"
  end
end

class SuperAnnouncer
  prepend SuperLoudAnnounce
end
