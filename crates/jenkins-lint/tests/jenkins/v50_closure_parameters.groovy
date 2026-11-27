pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    def a = { Map opts = [:] -> opts }
                    def b = { x, y = helper(1, 2) -> x }
                    def c = { n = -1 -> n }
                    def d = { String... rest -> rest.size() }
                    def e = { final String s, int i = 0 -> s }
                    def f = { List<Map<String, String>> rows, String[] names -> rows }
                    def g = { p = [1, 2], q = (p ? 'a' : 'b') -> q }
                    [1, 2].each {
                        item ->
                        echo "${item}"
                    }
                    [a: 1].each { k,
                                   v ->
                        echo "${k}=${v}"
                    }
                }
            }
        }
    }
}
