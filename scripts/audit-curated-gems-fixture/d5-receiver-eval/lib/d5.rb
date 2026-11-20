# Item 5: a string `eval` with a receiver is still a string eval.
Kernel.eval("class Integer; def d5_kernel = 1; end")
binding.eval("def d5_binding = 1")
