# Item 1: `instance_exec` on a chain receiver can define a method there.
String.instance_exec { attr_reader :d1_reader }
