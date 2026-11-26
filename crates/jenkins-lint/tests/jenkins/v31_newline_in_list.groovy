pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    def l = [env.A
                        + 'x', 'y']
                }
            }
        }
    }
}
