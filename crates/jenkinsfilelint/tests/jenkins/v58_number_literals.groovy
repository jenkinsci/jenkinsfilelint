pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    def a = 0xFF
                    def b = 0XAB
                    def c = 1_000
                    def d = 1.5e3
                    def e = 10L
                    def f = 1.5f
                    def g = 2G
                    def h = 017
                }
            }
        }
    }
}
