pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    def n = null
                    n << 'a.b.C' {
                        x()
                    }
                }
            }
        }
    }
}
