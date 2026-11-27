def f(String n) {
    sh script: "find -name "${n}" | egrep ."
}
