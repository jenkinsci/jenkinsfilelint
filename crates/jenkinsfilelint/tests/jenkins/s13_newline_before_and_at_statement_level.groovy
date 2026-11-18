pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    def x = env.A
                        && env.B
                }
            }
        }
    }
}
