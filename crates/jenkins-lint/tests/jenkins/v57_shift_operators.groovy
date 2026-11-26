pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    def a = 8 >> 1
                    def b = 8 >>> 2
                    def c = 1 << 3
                    int d = 16
                    d >>= 1
                    d <<= 2
                    d >>>= 1
                }
            }
        }
    }
}
