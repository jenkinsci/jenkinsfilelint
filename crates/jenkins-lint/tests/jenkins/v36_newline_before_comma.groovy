pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                script {
                    def l = ['a'
                        , 'b']
                }
            }
        }
    }
}
