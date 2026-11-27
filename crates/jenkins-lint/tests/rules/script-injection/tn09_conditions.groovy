pipeline {
    agent any
    stages {
        stage('Deploy') {
            when {
                expression { env.CHANGE_TITLE.contains('[deploy]') }
                environment name: 'TARGET', value: "${params.TARGET}"
            }
            steps {
                sh './deploy.sh'
            }
        }
    }
}
