[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE-MIT)
[![LICENSE](https://img.shields.io/badge/license-apache-blue.svg)](LICENSE-APACHE)
[![Documentation](https://docs.rs/static_init/badge.svg)](https://docs.rs/static_init)
[![Crates.io Version](https://img.shields.io/crates/v/static_init.svg)](https://crates.io/crates/static_init)

 Module initialization termination function with priorities and (mutable) statics initialization with
 non const functions.

 Minimum rust version required: 1.49
 

 # Functionalities

 - [x] Code execution before or after `main` but after libc and rust runtime has been initialized.

 - [x] Mutable and const statics with non const initialization.

 - [x] Statics dropable after `main` exits.

 - [x] Zero cost access to statics.

 - [x] Priorities on elf plateforms (linux, bsd, etc...) and window.

 # Example
 ```rust
 use static_init::{constructor,destructor,dynamic};

 #[constructor]
 fn do_init(){
 }
 //Care not to use priorities bellow 100
 //as those high priorities are used by
 //the rust runtime. (The lower the number
 //the higher the priority)
 #[constructor(200)]
 fn do_first(){
 }

 #[destructor]
 fn finaly() {
 }
 #[destructor(0)]
 fn ultimately() {
 }

 #[dynamic]
 static V: Vec<i32> = vec![1,2,3];

 #[dynamic(init,drop)]
 static mut V1: Vec<i32> = vec![1,2,3];

 //Initialized before V1 
 //then destroyed after V1 
 #[dynamic(init=142,drop=142)]
 static mut INIT_AND_DROP: Vec<i32> = vec![1,2,3];

 fn main(){
     assert_eq!(V[0],1);
     unsafe{
     assert_eq!(V1[2],3);
     V1[2] = 42;
     assert_eq!(V1[2], 42);
     }
 }
 ```

 # Attributes

 All functions marked with the `constructor` attribute are 
 run before `main` is started.

 All function marked with the `destructor` attribute are 
 run after `main` has returned.

 Static variables marked with the `dynamic` attribute can
 be initialized before main start and optionaly droped
 after main returns. 

 The attributes `constructor` and `destructor` works by placing the marked function pointer in
 dedicated object file sections. 

 Priority ranges from 0 to 2<sup>16</sup>-1. The absence of priority is equivalent to
 2<sup>16</sup>. 

 During program initialization:
     - constructors with priority 0 are the first called;
     - constructors without priority are called last.

 During program termination, the order is reversed:
     - destructors without priority are the first called;
     - destructors with priority 0 are the last called.
 
 # Comparisons against other crates

 ## [lazy_static][1]
  - lazy_static only provides const statics.
  - Each access to lazy_static statics costs 2ns on a x86.
  - lazy_static does not provide priorities.

 ## [ctor][2]
  - ctor only provides const statics.
  - ctor does not provide priorities.

 # Documentation and details

 ## Mac
   - [MACH_O specification](https://www.cnblogs.com/sunkang/archive/2011/05/24/2055635.html)
   - GCC source code gcc/config/darwin.c indicates that priorities are not supported. 

   Initialization functions pointers are placed in section "__DATA,__mod_init_func" and
   "__DATA,__mod_term_func"

 ## ELF plateforms:
  - `info ld`
  - linker script: `ld --verbose`
  - [ELF specification](https://docs.oracle.com/cd/E23824_01/html/819-0690/chapter7-1.html#scrolltoc)

  The runtime will run fonctions pointers of section ".init_array" at startup and function
  pointers in ".fini_array" at program exit. The linker place in the target object file
  sectio .init_array all sections from the source objects whose name is of the form
  .init_array.NNNNN in lexicographical order then the .init_array sections of those same source
  objects. It does equivalently with .fini_array and .fini_array.NNNN sections.

  Usage can be seen in gcc source gcc/config/pru.c

  Resources of libstdc++ are initialized with priority 100 (see gcc source libstdc++-v3/c++17/default_resource.h)
  The rust standard library function that capture the environment and executable arguments is
  executed at priority 99. Some callbacks constructors and destructors with priority 0 are
  registered by rust/rtlibrary.
  Static C++ objects are usually initialized with no priority (TBC). lib-c resources are
  initialized by the C-runtime before any function in the init_array (whatever the priority) are executed.

 ## Windows

  - [this blog post](https://www.cnblogs.com/sunkang/archive/2011/05/24/2055635.html)

  At start up, any functions pointer between sections ".CRT$XIA" and ".CRT$XIZ"
  and then any functions between ".CRT$XCA" and ".CRT$XCZ". It happens that the C library
  initialization functions pointer are placed in ".CRT$XIU" and C++ statics functions initialization
  pointers are placed in ".CRT$XCU". At program finish the pointers between sections
  ".CRT$XPA" and ".CRT$XPZ" are run first then those between ".CRT$XTA" and ".CRT$XTZ".

  Some reverse engineering was necessary to find out a way to implement 
  constructor/destructor priority.

  Contrarily to what is reported in this blog post, msvc linker
  only performs a lexicographicall ordering of section whose name
  is of the form "\<prefix\>$\<suffix\>" and have the same \<prefix\>.
  For example "RUST$01" and "RUST$02" will be ordered but those two
  sections will not be ordered with "RHUM" section.

  Moreover, it seems that section name of the form \<prefix\>$\<suffix\> are 
  not limited to 8 characters.

  So static initialization function pointers are placed in section ".CRT$XCU" and
  those with a priority `p` in `format!(".CRT$XCTZ{:05}",p)`. Destructors without priority
  are placed in ".CRT$XPU" and those with a priority in `format!(".CRT$XPTZ{:05}")`.


 [1]: https://crates.io/crates/lazy_static
 [2]: https://crates.io/crates/ctor
